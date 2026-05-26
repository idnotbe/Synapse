use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rmcp::ErrorData;
use synapse_core::{EventFilter, error_codes};
use synapse_perception::ObservationAssembler;
use tokio::{
    fs::{self, File},
    io::{AsyncWrite, AsyncWriteExt, BufWriter},
    time::{Instant, sleep},
};

use crate::{
    http::sse::SseState,
    m1::{ObserveParams, SharedM1State, mcp_error, observe_include},
    m3::permissions::{normalize_replay_path, replay_root},
};

use super::{
    EVENT_DRAIN_INTERVAL, OBSERVATION_SAMPLE_INTERVAL, ReplayFormat, ReplayRecordParams,
    ReplayRecordResponse, ReplayTarget, events::drain_events, observations::ObservationWrite,
    observations::write_observation,
};

pub async fn record_replay(
    m1_state: SharedM1State,
    sse_state: SseState,
    params: &ReplayRecordParams,
) -> Result<ReplayRecordResponse, ErrorData> {
    let target = ReplayTarget::parse(&params.target)?;
    let _format = ReplayFormat::parse(&params.format)?;
    let path = replay_path(params.path.as_deref())?;
    create_parent_dir(&path).await?;

    let file = File::create(&path).await.map_err(|error| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            format!("replay_record could not create {}: {error}", path.display()),
        )
    })?;
    let mut writer = BufWriter::new(file);

    let stats = if params.duration_ms > 0 {
        record_window(&mut writer, &m1_state, &sse_state, target, params).await?
    } else {
        RecordWindowStats::default()
    };

    writer.flush().await.map_err(|error| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            format!("replay_record could not flush {}: {error}", path.display()),
        )
    })?;
    drop(writer);

    let bytes = fs::metadata(&path).await.map_err(|error| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            format!(
                "replay_record could not read metadata for {}: {error}",
                path.display()
            ),
        )
    })?;

    Ok(ReplayRecordResponse {
        path: display_path(&path),
        records_written: stats.records_written,
        observations_skipped: stats.observations_skipped,
        bytes: bytes.len(),
    })
}

async fn record_window<W>(
    writer: &mut W,
    m1_state: &SharedM1State,
    sse_state: &SseState,
    target: ReplayTarget,
    params: &ReplayRecordParams,
) -> Result<RecordWindowStats, ErrorData>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut stats = RecordWindowStats::default();
    let deadline = Instant::now() + Duration::from_millis(u64::from(params.duration_ms));
    let mut event_subscription = if target.includes_events() {
        Some(
            sse_state
                .event_bus()
                .subscribe(EventFilter::All, Vec::new(), false)
                .map_err(|error| mcp_error(error.code(), error.to_string()))?,
        )
    } else {
        None
    };
    let event_bus = sse_state.event_bus();
    let assembler = ObservationAssembler::new();
    let include = observe_include(&ObserveParams::default());
    let mut next_observation_sample = Instant::now();

    let result = async {
        if target.includes_observations() {
            stats.record_observation(
                write_observation(writer, m1_state, &assembler, include, target).await?,
            );
            next_observation_sample = Instant::now() + OBSERVATION_SAMPLE_INTERVAL;
        }

        while Instant::now() < deadline {
            if let Some(subscription) = &event_subscription {
                stats.records_written = stats
                    .records_written
                    .saturating_add(drain_events(writer, subscription.drain(), target).await?);
            }

            if target.includes_observations() && Instant::now() >= next_observation_sample {
                stats.record_observation(
                    write_observation(writer, m1_state, &assembler, include, target).await?,
                );
                next_observation_sample += OBSERVATION_SAMPLE_INTERVAL;
            }

            sleep(next_sleep(deadline, next_observation_sample, target)).await;
        }

        if let Some(subscription) = &event_subscription {
            stats.records_written = stats
                .records_written
                .saturating_add(drain_events(writer, subscription.drain(), target).await?);
        }

        if target.includes_observations()
            && stats.observations_written == 0
            && stats.observations_skipped > 0
        {
            return Err(mcp_error(
                error_codes::OBSERVE_NO_PERCEPTION_AVAILABLE,
                format!(
                    "replay_record could not capture any observations; skipped {} unavailable samples",
                    stats.observations_skipped
                ),
            ));
        }

        if stats.observations_skipped > 0 {
            tracing::warn!(
                code = "REPLAY_OBSERVATION_GAPS_SKIPPED",
                observations_skipped = stats.observations_skipped,
                observations_written = stats.observations_written,
                "replay_record skipped transient unavailable observation samples"
            );
        }

        Ok(stats)
    }
    .await;

    if let Some(subscription) = event_subscription.take() {
        event_bus.unsubscribe(subscription.id());
    }

    result
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct RecordWindowStats {
    records_written: u64,
    observations_written: u64,
    observations_skipped: u64,
}

impl RecordWindowStats {
    const fn record_observation(&mut self, write: ObservationWrite) {
        self.records_written = self.records_written.saturating_add(write.records_written);
        self.observations_written = self
            .observations_written
            .saturating_add(write.observations_written);
        self.observations_skipped = self
            .observations_skipped
            .saturating_add(write.observations_skipped);
    }
}

async fn create_parent_dir(path: &Path) -> Result<(), ErrorData> {
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).await.map_err(|error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!(
                    "replay_record could not create parent directory {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    Ok(())
}

fn replay_path(path: Option<&str>) -> Result<PathBuf, ErrorData> {
    normalize_replay_path(&replay_root(), path)
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn next_sleep(
    deadline: Instant,
    next_observation_sample: Instant,
    target: ReplayTarget,
) -> Duration {
    let now = Instant::now();
    if now >= deadline {
        return Duration::ZERO;
    }
    let until_deadline = deadline.saturating_duration_since(now);
    let base = until_deadline.min(EVENT_DRAIN_INTERVAL);
    if target.includes_observations() {
        base.min(next_observation_sample.saturating_duration_since(now))
    } else {
        base
    }
}
