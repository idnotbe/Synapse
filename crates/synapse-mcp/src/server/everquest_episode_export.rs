#![allow(clippy::derive_partial_eq_without_eq)]

use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use rmcp::{ErrorData, schemars::JsonSchema};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use synapse_core::error_codes;

use super::{
    Json, Parameters, SynapseService,
    everquest_domain::{
        EverQuestDynamicJepaActionRow, EverQuestDynamicJepaOutcomeRow,
        EverQuestDynamicJepaStateRow, EverQuestDynamicJepaTransitionRow,
    },
    everquest_log::EVERQUEST_PROFILE_ID,
    everquest_trajectory::{EverQuestTrajectoryRow, EverQuestTrajectoryTransitionRow},
    tool, tool_router,
};
use crate::m1::mcp_error;

const TOOL: &str = "everquest_episode_export";
const SCHEMA_VERSION: u32 = 1;
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 512;
const MAX_TRAJECTORY_ROWS: usize = 32;
const MAX_ISSUE_REFS: usize = 64;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestEpisodeExportParams {
    pub export_id: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    pub trajectory_row_keys: Vec<String>,
    #[serde(default)]
    pub issue_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestEpisodeExportResponse {
    pub ok: bool,
    pub export_id: String,
    pub profile_id: String,
    pub output_path: String,
    pub line_count: u32,
    pub bytes: u64,
    pub sha256: String,
    pub episode_count: u32,
    pub source_row_count: u32,
    pub readback: EverQuestEpisodeJsonlReadback,
    pub episodes: Vec<EverQuestEpisodeExportSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestEpisodeExportSummary {
    pub episode_id: String,
    pub trajectory_row_key: String,
    pub transition_id: String,
    pub state_row_key: String,
    pub action_row_key: String,
    pub outcome_row_key: String,
    pub domain_transition_row_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestEpisodeJsonlReadback {
    pub line_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_episode_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_episode_id: Option<String>,
    pub sha256: String,
    pub bytes: u64,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestEpisodeRedaction {
    pub compact_redacted: bool,
    pub raw_chat_body_persisted: bool,
    pub raw_target_names_persisted: bool,
    pub raw_session_id_persisted: bool,
    pub private_session_id_hash_only: bool,
    pub all_log_refs_marked_redacted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestContextGraphEpisodeRow {
    pub schema_version: u32,
    pub record_kind: String,
    pub episode_id: String,
    pub export_id: String,
    pub profile_id: String,
    pub contextgraph: Value,
    pub source_of_truth: Value,
    pub state: Value,
    pub action: Value,
    pub outcome: Value,
    pub transition: Value,
    pub expected_persisted_delta: Value,
    pub actual_readback: Value,
    pub redaction: EverQuestEpisodeRedaction,
    pub evidence_boundary: Value,
}

#[derive(Clone, Debug)]
struct NormalizedExportParams {
    export_id: String,
    profile_id: String,
    trajectory_row_keys: Vec<String>,
    issue_refs: Vec<String>,
    output_path: PathBuf,
    overwrite: bool,
}

#[derive(Clone, Debug)]
struct ReadRow<T> {
    key: String,
    value_len_bytes: u64,
    sha256: String,
    row: T,
}

#[derive(Clone, Debug)]
struct EpisodeBuild {
    row: EverQuestContextGraphEpisodeRow,
    summary: EverQuestEpisodeExportSummary,
    source_row_count: usize,
}

#[derive(Clone, Debug)]
struct EpisodeSources {
    domain_transition: ReadRow<EverQuestDynamicJepaTransitionRow>,
    state: ReadRow<EverQuestDynamicJepaStateRow>,
    action: ReadRow<EverQuestDynamicJepaActionRow>,
    outcome: ReadRow<EverQuestDynamicJepaOutcomeRow>,
    current_state: Value,
}

#[tool_router(router = everquest_episode_export_tool_router, vis = "pub(super)")]
impl SynapseService {
    #[tool(
        description = "Export redacted EverQuest trajectory/domain rows to ContextGraph-compatible DynamicJEPA episode JSONL with final artifact readback"
    )]
    pub async fn everquest_episode_export(
        &self,
        params: Parameters<EverQuestEpisodeExportParams>,
    ) -> Result<Json<EverQuestEpisodeExportResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = TOOL,
            "tool.invocation kind=everquest_episode_export"
        );
        let params = normalize_params(&params.0)?;
        let response = self.export_episodes(params)?;
        Ok(Json(response))
    }
}

impl SynapseService {
    fn export_episodes(
        &self,
        params: NormalizedExportParams,
    ) -> Result<EverQuestEpisodeExportResponse, ErrorData> {
        let builds = self.build_episode_export_rows(&params)?;
        if builds.is_empty() {
            return Err(params_error(
                "EverQuest episode export produced zero rows; refusing empty JSONL",
            ));
        }
        let rows = builds
            .iter()
            .map(|build| build.row.clone())
            .collect::<Vec<_>>();
        let readback = write_jsonl_artifact(&params.output_path, &rows, params.overwrite)?;
        Ok(EverQuestEpisodeExportResponse {
            ok: true,
            export_id: params.export_id,
            profile_id: params.profile_id,
            output_path: params.output_path.display().to_string(),
            line_count: readback.line_count,
            bytes: readback.bytes,
            sha256: readback.sha256.clone(),
            episode_count: len_to_u32(rows.len()),
            source_row_count: len_to_u32(
                builds
                    .iter()
                    .map(|build| build.source_row_count)
                    .sum::<usize>(),
            ),
            readback,
            episodes: builds.into_iter().map(|build| build.summary).collect(),
        })
    }

    fn build_episode_export_rows(
        &self,
        params: &NormalizedExportParams,
    ) -> Result<Vec<EpisodeBuild>, ErrorData> {
        let runtime = self.reflex_runtime()?;
        let runtime = runtime.lock().map_err(|_| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                "reflex runtime lock poisoned while exporting EverQuest episodes",
            )
        })?;
        let mut rows = Vec::new();
        let mut episode_ids = HashSet::new();
        for trajectory_key in &params.trajectory_row_keys {
            let trajectory =
                read_required_json_row::<EverQuestTrajectoryRow>(&runtime, trajectory_key)?;
            validate_trajectory_row(&trajectory, &params.profile_id)?;
            for transition in &trajectory.row.transitions {
                let build = build_episode_row(&runtime, params, &trajectory, transition)?;
                if !episode_ids.insert(build.row.episode_id.clone()) {
                    return Err(params_error(format!(
                        "duplicate episode_id generated: {}",
                        build.row.episode_id
                    )));
                }
                rows.push(build);
            }
        }
        drop(runtime);
        Ok(rows)
    }
}

fn build_episode_row(
    runtime: &synapse_reflex::ReflexRuntime,
    params: &NormalizedExportParams,
    trajectory: &ReadRow<EverQuestTrajectoryRow>,
    trajectory_transition: &EverQuestTrajectoryTransitionRow,
) -> Result<EpisodeBuild, ErrorData> {
    let sources = read_episode_sources(runtime, params, trajectory_transition)?;
    let episode_id = format!(
        "{}.{}",
        trajectory.row.trajectory_id, trajectory_transition.transition_id
    );
    let row = EverQuestContextGraphEpisodeRow {
        schema_version: SCHEMA_VERSION,
        record_kind: "everquest_contextgraph_dynamicjepa_episode".to_owned(),
        episode_id: episode_id.clone(),
        export_id: params.export_id.clone(),
        profile_id: params.profile_id.clone(),
        contextgraph: contextgraph_block(),
        source_of_truth: source_of_truth_block(trajectory, trajectory_transition, &sources, params),
        state: state_block(&sources.state),
        action: action_block(&sources.action),
        outcome: outcome_block(&sources.outcome),
        transition: transition_block(&sources.domain_transition, trajectory_transition),
        expected_persisted_delta: json!({
            "artifact_path": params.output_path.display().to_string(),
            "record_kind": "everquest_contextgraph_dynamicjepa_episode",
            "episode_id": episode_id,
            "line_format": "jsonl",
            "empty_export_refused": true
        }),
        actual_readback: json!({
            "source": "synapse_cf_kv",
            "source_rows_read_before_export": true,
            "source_row_count": 6,
            "artifact_final_readback": "reported_by_tool_response",
            "trajectory_sha256": trajectory.sha256
        }),
        redaction: redaction_block(),
        evidence_boundary: json!({
            "manual_fsv_required_for_runtime": true,
            "is_training_script": false,
            "exports_real_synapse_rows": true,
            "raw_chat_or_session_data_excluded": true,
            "note": "Episode export is compact planning/model evidence; gameplay claims still require attended manual UI/log/storage FSV."
        }),
    };
    Ok(EpisodeBuild {
        summary: EverQuestEpisodeExportSummary {
            episode_id: row.episode_id.clone(),
            trajectory_row_key: trajectory.key.clone(),
            transition_id: trajectory_transition.transition_id.clone(),
            state_row_key: sources.state.key,
            action_row_key: sources.action.key,
            outcome_row_key: sources.outcome.key,
            domain_transition_row_key: sources.domain_transition.key,
        },
        row,
        source_row_count: 6,
    })
}

fn read_episode_sources(
    runtime: &synapse_reflex::ReflexRuntime,
    params: &NormalizedExportParams,
    trajectory_transition: &EverQuestTrajectoryTransitionRow,
) -> Result<EpisodeSources, ErrorData> {
    let domain_transition_key = trajectory_transition
        .domain_transition_row_key
        .as_deref()
        .ok_or_else(|| {
            params_error(format!(
                "trajectory transition {} is missing domain_transition_row_key",
                trajectory_transition.transition_id
            ))
        })?;
    let domain_transition = read_required_json_row::<EverQuestDynamicJepaTransitionRow>(
        runtime,
        domain_transition_key,
    )?;
    validate_domain_transition(
        &domain_transition,
        &params.profile_id,
        trajectory_transition,
    )?;
    let state = read_required_json_row::<EverQuestDynamicJepaStateRow>(
        runtime,
        &domain_transition.row.state_row_key,
    )?;
    let action = read_required_json_row::<EverQuestDynamicJepaActionRow>(
        runtime,
        &domain_transition.row.action_row_key,
    )?;
    let outcome = read_required_json_row::<EverQuestDynamicJepaOutcomeRow>(
        runtime,
        &domain_transition.row.outcome_row_key,
    )?;
    validate_domain_links(&domain_transition, &state, &action, &outcome)?;

    let current_state_bytes = read_required_bytes(runtime, &trajectory_transition.state_row_key)?;
    let current_state = json!({
        "cf_name": "CF_KV",
        "row_key": trajectory_transition.state_row_key,
        "value_len_bytes": len_to_u64(current_state_bytes.len()),
        "sha256": sha256_hex(&current_state_bytes)
    });
    Ok(EpisodeSources {
        domain_transition,
        state,
        action,
        outcome,
        current_state,
    })
}

fn source_of_truth_block(
    trajectory: &ReadRow<EverQuestTrajectoryRow>,
    trajectory_transition: &EverQuestTrajectoryTransitionRow,
    sources: &EpisodeSources,
    params: &NormalizedExportParams,
) -> Value {
    json!({
        "synapse_storage": {
            "trajectory": source_row_json(trajectory),
            "current_state": sources.current_state,
            "dynamicjepa_transition": source_row_json(&sources.domain_transition),
            "dynamicjepa_state": source_row_json(&sources.state),
            "dynamicjepa_action": source_row_json(&sources.action),
            "dynamicjepa_outcome": source_row_json(&sources.outcome)
        },
        "storage_refs": {
            "action_refs": trajectory_transition.action_refs,
            "observation_refs": trajectory_transition.observation_refs,
            "event_refs": trajectory_transition.event_refs
        },
        "log_refs": trajectory_transition.log_refs,
        "trajectory_source_refs": trajectory.row.source_refs,
        "domain_source_refs": sources.domain_transition.row.source_refs,
        "issue_refs": params.issue_refs
    })
}

const fn redaction_block() -> EverQuestEpisodeRedaction {
    EverQuestEpisodeRedaction {
        compact_redacted: true,
        raw_chat_body_persisted: false,
        raw_target_names_persisted: false,
        raw_session_id_persisted: false,
        private_session_id_hash_only: true,
        all_log_refs_marked_redacted: true,
    }
}

fn validate_trajectory_row(
    trajectory: &ReadRow<EverQuestTrajectoryRow>,
    profile_id: &str,
) -> Result<(), ErrorData> {
    if trajectory.row.schema_version != SCHEMA_VERSION
        || trajectory.row.row_kind != "everquest_trajectory"
    {
        return Err(mcp_error(
            error_codes::STORAGE_CORRUPTED,
            format!(
                "trajectory row has invalid schema or row_kind: {}",
                trajectory.key
            ),
        ));
    }
    if trajectory.row.profile_id != profile_id {
        return Err(params_error(format!(
            "trajectory row profile_id mismatch: expected {profile_id}, got {}",
            trajectory.row.profile_id
        )));
    }
    if trajectory.row.redaction.raw_chat_body_persisted
        || trajectory.row.redaction.raw_target_names_persisted
        || !trajectory.row.redaction.compact_redacted
        || !trajectory.row.redaction.all_log_refs_marked_redacted
    {
        return Err(params_error(format!(
            "trajectory row is not exportable because redaction is incomplete: {}",
            trajectory.key
        )));
    }
    for transition in &trajectory.row.transitions {
        if transition.log_refs.iter().any(|log_ref| !log_ref.redacted) {
            return Err(params_error(format!(
                "trajectory transition {} has unredacted log refs",
                transition.transition_id
            )));
        }
    }
    Ok(())
}

fn validate_domain_transition(
    domain_transition: &ReadRow<EverQuestDynamicJepaTransitionRow>,
    profile_id: &str,
    trajectory_transition: &EverQuestTrajectoryTransitionRow,
) -> Result<(), ErrorData> {
    if domain_transition.row.schema_version != SCHEMA_VERSION
        || domain_transition.row.row_kind != "everquest_dynamicjepa_transition"
    {
        return Err(mcp_error(
            error_codes::STORAGE_CORRUPTED,
            format!(
                "domain transition row has invalid schema or row_kind: {}",
                domain_transition.key
            ),
        ));
    }
    if domain_transition.row.profile_id != profile_id {
        return Err(params_error(format!(
            "domain transition profile_id mismatch: expected {profile_id}, got {}",
            domain_transition.row.profile_id
        )));
    }
    if domain_transition.row.transition_id != trajectory_transition.transition_id {
        return Err(mcp_error(
            error_codes::STORAGE_CORRUPTED,
            "trajectory/domain transition id mismatch",
        ));
    }
    if domain_transition
        .row
        .evidence_boundary
        .raw_chat_body_persisted
        || !domain_transition.row.evidence_boundary.compact_redacted
    {
        return Err(params_error(format!(
            "domain transition row is not exportable because redaction is incomplete: {}",
            domain_transition.key
        )));
    }
    Ok(())
}

fn validate_domain_links(
    transition: &ReadRow<EverQuestDynamicJepaTransitionRow>,
    state: &ReadRow<EverQuestDynamicJepaStateRow>,
    action: &ReadRow<EverQuestDynamicJepaActionRow>,
    outcome: &ReadRow<EverQuestDynamicJepaOutcomeRow>,
) -> Result<(), ErrorData> {
    if state.row.row_key != transition.row.state_row_key
        || action.row.row_key != transition.row.action_row_key
        || outcome.row.row_key != transition.row.outcome_row_key
        || state.row.transition_id != transition.row.transition_id
        || action.row.transition_id != transition.row.transition_id
        || outcome.row.transition_id != transition.row.transition_id
    {
        return Err(mcp_error(
            error_codes::STORAGE_CORRUPTED,
            "DynamicJEPA state/action/outcome linkage mismatch",
        ));
    }
    Ok(())
}

fn state_block(row: &ReadRow<EverQuestDynamicJepaStateRow>) -> Value {
    json!({
        "record_kind": "NormalizedState",
        "row_key": row.key,
        "state_id": row.row.state_id,
        "transition_id": row.row.transition_id,
        "fields": row.row.fields,
        "field_values": row.row.field_values,
        "source_refs": row.row.source_refs
    })
}

fn action_block(row: &ReadRow<EverQuestDynamicJepaActionRow>) -> Value {
    json!({
        "record_kind": "NormalizedAction",
        "row_key": row.key,
        "action_id": row.row.action_id,
        "transition_id": row.row.transition_id,
        "fields": row.row.fields,
        "field_values": row.row.field_values,
        "source_refs": row.row.source_refs
    })
}

fn outcome_block(row: &ReadRow<EverQuestDynamicJepaOutcomeRow>) -> Value {
    json!({
        "record_kind": "NormalizedOutcome",
        "row_key": row.key,
        "outcome_id": row.row.outcome_id,
        "transition_id": row.row.transition_id,
        "fields": row.row.fields,
        "field_values": row.row.field_values,
        "source_refs": row.row.source_refs
    })
}

fn transition_block(
    row: &ReadRow<EverQuestDynamicJepaTransitionRow>,
    trajectory_transition: &EverQuestTrajectoryTransitionRow,
) -> Value {
    json!({
        "record_kind": "StateTransition",
        "row_key": row.key,
        "transition_id": row.row.transition_id,
        "prior_state": row.row.prior_state_id,
        "action": row.row.action_id,
        "outcome": row.row.outcome_id,
        "next_state": row.row.next_state_id,
        "timestamp_ms": trajectory_transition.occurred_at.timestamp_millis(),
        "validation_status": row.row.validation_status,
        "accepted_for_planning": row.row.accepted_for_planning,
        "invariant_results": row.row.invariant_results,
        "rejection_reasons": row.row.rejection_reasons,
        "planner_policy": row.row.planner_policy,
        "entity": {
            "character_summary": row.row.entity.character_summary,
            "server": row.row.entity.server,
            "trajectory_id": row.row.entity.trajectory_id,
            "session_id_sha256": sha256_hex(row.row.entity.session_id.as_bytes())
        }
    })
}

fn contextgraph_block() -> Value {
    json!({
        "format": "dynamicjepa_episode_jsonl",
        "compatible": true,
        "source_repo": "ChrisRoyse/contextgraph",
        "compatible_contextgraph_cfs": [
            "dj_domain_packs",
            "dj_raw_events",
            "dj_normalized_states",
            "dj_actions",
            "dj_outcomes",
            "dj_transitions",
            "dj_trajectories",
            "dj_verification_runs",
            "dj_audit_log"
        ],
        "episode_blocks": [
            "source_of_truth",
            "state",
            "action",
            "outcome",
            "expected_persisted_delta",
            "actual_readback"
        ]
    })
}

fn source_row_json<T>(row: &ReadRow<T>) -> Value {
    json!({
        "cf_name": "CF_KV",
        "row_key": row.key,
        "value_len_bytes": row.value_len_bytes,
        "sha256": row.sha256
    })
}

fn read_required_json_row<T>(
    runtime: &synapse_reflex::ReflexRuntime,
    key: &str,
) -> Result<ReadRow<T>, ErrorData>
where
    T: DeserializeOwned,
{
    let bytes = read_required_bytes(runtime, key)?;
    let row = serde_json::from_slice::<T>(&bytes).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_CORRUPTED,
            format!("decode CF_KV row {key}: {error}"),
        )
    })?;
    Ok(ReadRow {
        key: key.to_owned(),
        value_len_bytes: len_to_u64(bytes.len()),
        sha256: sha256_hex(&bytes),
        row,
    })
}

fn read_required_bytes(
    runtime: &synapse_reflex::ReflexRuntime,
    key: &str,
) -> Result<Vec<u8>, ErrorData> {
    runtime
        .storage_kv_row(key.as_bytes())
        .map_err(|error| mcp_error(error.code(), error.to_string()))?
        .ok_or_else(|| {
            mcp_error(
                error_codes::STORAGE_READ_FAILED,
                format!("required CF_KV row missing: {key}"),
            )
        })
}

fn write_jsonl_artifact(
    path: &Path,
    rows: &[EverQuestContextGraphEpisodeRow],
    overwrite: bool,
) -> Result<EverQuestEpisodeJsonlReadback, ErrorData> {
    let parent = path
        .parent()
        .ok_or_else(|| params_error("output_path missing parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_WRITE_FAILED,
            format!("create EverQuest episode export dir: {error}"),
        )
    })?;
    if path.exists() && !overwrite {
        return Err(params_error(format!(
            "output_path already exists; set overwrite=true or choose another export_id: {}",
            path.display()
        )));
    }
    let temp_path = temp_path(path)?;
    if temp_path.exists() {
        fs::remove_file(&temp_path).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_WRITE_FAILED,
                format!("remove stale EverQuest episode temp file: {error}"),
            )
        })?;
    }
    let mut bytes = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut bytes, row).map_err(|error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!("encode EverQuest episode JSONL row: {error}"),
            )
        })?;
        bytes.push(b'\n');
    }
    fs::write(&temp_path, &bytes).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_WRITE_FAILED,
            format!("write EverQuest episode temp JSONL: {error}"),
        )
    })?;
    let temp_readback = fs::read(&temp_path).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_READ_FAILED,
            format!("read EverQuest episode temp JSONL: {error}"),
        )
    })?;
    if temp_readback != bytes {
        return Err(mcp_error(
            error_codes::STORAGE_CORRUPTED,
            "EverQuest episode temp JSONL readback mismatch",
        ));
    }
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_WRITE_FAILED,
                format!("replace EverQuest episode JSONL: {error}"),
            )
        })?;
    }
    fs::rename(&temp_path, path).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_WRITE_FAILED,
            format!("promote EverQuest episode JSONL: {error}"),
        )
    })?;
    readback_jsonl(path)
}

fn readback_jsonl(path: &Path) -> Result<EverQuestEpisodeJsonlReadback, ErrorData> {
    let bytes = fs::read(path).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_READ_FAILED,
            format!("read EverQuest episode JSONL: {error}"),
        )
    })?;
    let mut line_count = 0_u32;
    let mut first_episode_id = None;
    let mut last_episode_id = None;
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value = serde_json::from_slice::<Value>(line).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_CORRUPTED,
                format!("decode EverQuest episode JSONL readback: {error}"),
            )
        })?;
        let episode_id = value
            .get("episode_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                mcp_error(
                    error_codes::STORAGE_CORRUPTED,
                    "EverQuest episode JSONL row missing episode_id",
                )
            })?
            .to_owned();
        if first_episode_id.is_none() {
            first_episode_id = Some(episode_id.clone());
        }
        last_episode_id = Some(episode_id);
        line_count = line_count.saturating_add(1);
    }
    Ok(EverQuestEpisodeJsonlReadback {
        line_count,
        first_episode_id,
        last_episode_id,
        sha256: sha256_hex(&bytes),
        bytes: len_to_u64(bytes.len()),
    })
}

fn normalize_params(
    params: &EverQuestEpisodeExportParams,
) -> Result<NormalizedExportParams, ErrorData> {
    let export_id = validate_id("export_id", &params.export_id)?;
    let profile_id = validate_profile_id(&params.profile_id)?;
    if params.trajectory_row_keys.is_empty() {
        return Err(params_error("trajectory_row_keys must not be empty"));
    }
    if params.trajectory_row_keys.len() > MAX_TRAJECTORY_ROWS {
        return Err(params_error(format!(
            "trajectory_row_keys must contain <= {MAX_TRAJECTORY_ROWS} rows"
        )));
    }
    let mut seen = BTreeSet::new();
    let trajectory_row_keys = params
        .trajectory_row_keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let key = normalize_required_text(&format!("trajectory_row_keys[{index}]"), key)?;
            if !seen.insert(key.clone()) {
                return Err(params_error(format!("duplicate trajectory row key: {key}")));
            }
            Ok(key)
        })
        .collect::<Result<Vec<_>, ErrorData>>()?;
    let issue_refs = normalize_issue_refs(&params.issue_refs)?;
    let output_path =
        normalize_output_path(&profile_id, &export_id, params.output_path.as_deref())?;
    Ok(NormalizedExportParams {
        export_id,
        profile_id,
        trajectory_row_keys,
        issue_refs,
        output_path,
        overwrite: params.overwrite,
    })
}

fn normalize_issue_refs(values: &[String]) -> Result<Vec<String>, ErrorData> {
    if values.len() > MAX_ISSUE_REFS {
        return Err(params_error(format!(
            "issue_refs must contain <= {MAX_ISSUE_REFS} values"
        )));
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| normalize_required_text(&format!("issue_refs[{index}]"), value))
        .collect()
}

fn normalize_output_path(
    profile_id: &str,
    export_id: &str,
    output_path: Option<&str>,
) -> Result<PathBuf, ErrorData> {
    let root = episode_export_root().join(profile_id);
    let relative = output_path.map_or_else(
        || Ok(PathBuf::from(format!("{export_id}.jsonl"))),
        |value| {
            let value = normalize_required_text("output_path", value)?;
            let path = PathBuf::from(value);
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(params_error(
                    "output_path must be a relative path below the EverQuest episode export root",
                ));
            }
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
            {
                return Err(params_error("output_path must end with .jsonl"));
            }
            Ok(path)
        },
    )?;
    Ok(root.join(relative))
}

fn validate_profile_id(value: &str) -> Result<String, ErrorData> {
    let value = value.trim();
    if value != EVERQUEST_PROFILE_ID {
        return Err(params_error(format!(
            "profile_id must be {EVERQUEST_PROFILE_ID:?}; got {value:?}"
        )));
    }
    Ok(value.to_owned())
}

fn validate_id(field: &str, value: &str) -> Result<String, ErrorData> {
    let value = normalize_required_text(field, value)?;
    if value.len() > MAX_ID_BYTES {
        return Err(params_error(format!(
            "{field} must be <= {MAX_ID_BYTES} bytes"
        )));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(params_error(format!(
            "{field} must contain only ASCII letters, digits, '.', '_', or '-'"
        )));
    }
    Ok(value)
}

fn normalize_required_text(field: &str, value: &str) -> Result<String, ErrorData> {
    let value = value.trim();
    if value.is_empty() {
        return Err(params_error(format!("{field} must not be empty")));
    }
    if value.len() > MAX_TEXT_BYTES {
        return Err(params_error(format!(
            "{field} must be <= {MAX_TEXT_BYTES} bytes"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(params_error(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(value.to_owned())
}

fn temp_path(path: &Path) -> Result<PathBuf, ErrorData> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| params_error("output_path missing file name"))?;
    Ok(path.with_file_name(format!("{file_name}.tmp")))
}

fn episode_export_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("synapse")
        .join("everquest")
        .join("contextgraph_episodes")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn len_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn len_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn params_error(message: impl Into<String>) -> ErrorData {
    mcp_error(error_codes::TOOL_PARAMS_INVALID, message)
}

fn default_profile_id() -> String {
    EVERQUEST_PROFILE_ID.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_trajectory_list() {
        let error = normalize_params(&EverQuestEpisodeExportParams {
            export_id: "issue521".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            trajectory_row_keys: Vec::new(),
            issue_refs: Vec::new(),
            output_path: None,
            overwrite: false,
        })
        .unwrap_err();
        assert_eq!(
            error.data.as_ref().and_then(|data| data.get("code")),
            Some(&serde_json::json!(error_codes::TOOL_PARAMS_INVALID))
        );
    }

    #[test]
    fn rejects_absolute_output_path() {
        let error = normalize_params(&EverQuestEpisodeExportParams {
            export_id: "issue521".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            trajectory_row_keys: vec!["everquest/trajectory/v1/everquest.live/t1".to_owned()],
            issue_refs: Vec::new(),
            output_path: Some("C:\\tmp\\bad.jsonl".to_owned()),
            overwrite: false,
        })
        .unwrap_err();
        assert_eq!(
            error.data.as_ref().and_then(|data| data.get("code")),
            Some(&serde_json::json!(error_codes::TOOL_PARAMS_INVALID))
        );
    }

    #[test]
    fn session_id_hash_is_deterministic() {
        assert_eq!(
            sha256_hex(b"issue521-session"),
            sha256_hex(b"issue521-session")
        );
        assert_ne!(
            sha256_hex(b"issue521-session"),
            sha256_hex(b"issue521-other-session")
        );
    }
}
