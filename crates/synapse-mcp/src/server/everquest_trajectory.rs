#![allow(clippy::derive_partial_eq_without_eq)]

use std::{
    collections::HashSet,
    fs,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

use chrono::{DateTime, Utc};
use rmcp::{ErrorData, schemars::JsonSchema};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use synapse_core::error_codes;
use synapse_storage::cf;

use super::{
    Json, Parameters, SynapseService, everquest_log::EVERQUEST_PROFILE_ID, tool, tool_router,
};
use crate::m1::mcp_error;

const TOOL: &str = "everquest_trajectory_record";
const SCHEMA_VERSION: u32 = 1;
const TRAJECTORY_ROW_PREFIX: &str = "everquest/trajectory/v1";
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 512;
const MAX_TRANSITIONS: usize = 32;
const MAX_REFS_PER_TRANSITION: usize = 32;
const MAX_SOURCE_REFS: usize = 32;
const HASH_BYTES: usize = 32;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryRecordParams {
    pub trajectory_id: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    pub intent: EverQuestTrajectoryIntent,
    pub session_id: String,
    pub transitions: Vec<EverQuestTrajectoryTransitionInput>,
    #[serde(default)]
    pub source_refs: Vec<EverQuestTrajectorySourceRef>,
    #[serde(default = "default_export_jsonl")]
    pub export_jsonl: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryRecordResponse {
    pub ok: bool,
    pub row_key: String,
    pub duplicate_of_prior_row: bool,
    pub stored_value_len_bytes: u64,
    pub summary: EverQuestTrajectorySummary,
    pub trajectory: EverQuestTrajectoryRow,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EverQuestTrajectoryIntent {
    NavigationProbe,
    TargetConsiderProbe,
    CombatAttempt,
    Recovery,
    LevelUpRun,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryTransitionInput {
    pub transition_id: String,
    pub sequence: u32,
    pub occurred_at: DateTime<Utc>,
    pub state_row_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_transition_row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard_row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_state_row_key: Option<String>,
    pub action_refs: Vec<EverQuestTrajectoryStorageRefInput>,
    pub observation_refs: Vec<EverQuestTrajectoryStorageRefInput>,
    pub event_refs: Vec<EverQuestTrajectoryStorageRefInput>,
    pub log_refs: Vec<EverQuestTrajectoryLogRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryStorageRefInput {
    pub cf_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_key_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryStorageRef {
    pub cf_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_key: Option<String>,
    pub row_key_hex: String,
    pub value_len_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryLogRef {
    pub path: String,
    pub start_offset: u64,
    pub next_offset: u64,
    pub event_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    pub redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectorySourceRef {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryRow {
    pub schema_version: u32,
    pub row_kind: String,
    pub profile_id: String,
    pub trajectory_id: String,
    pub row_key: String,
    pub intent: EverQuestTrajectoryIntent,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub trajectory_hash: String,
    pub transitions: Vec<EverQuestTrajectoryTransitionRow>,
    pub summary: EverQuestTrajectorySummary,
    pub source_refs: Vec<EverQuestTrajectorySourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<EverQuestTrajectoryExportArtifact>,
    pub redaction: EverQuestTrajectoryRedaction,
    pub evidence_boundary: EverQuestTrajectoryEvidenceBoundary,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryTransitionRow {
    pub transition_id: String,
    pub sequence: u32,
    pub occurred_at: DateTime<Utc>,
    pub state_row_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_transition_row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard_row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_state_row_key: Option<String>,
    pub linked_state_len_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_domain_transition_len_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_outcome_len_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_guard_len_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_map_state_len_bytes: Option<u64>,
    pub action_refs: Vec<EverQuestTrajectoryStorageRef>,
    pub observation_refs: Vec<EverQuestTrajectoryStorageRef>,
    pub event_refs: Vec<EverQuestTrajectoryStorageRef>,
    pub log_refs: Vec<EverQuestTrajectoryLogRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectorySummary {
    pub transition_count: u32,
    pub first_at: DateTime<Utc>,
    pub last_at: DateTime<Utc>,
    pub action_ref_count: u32,
    pub observation_ref_count: u32,
    pub event_ref_count: u32,
    pub log_ref_count: u32,
    pub state_ref_count: u32,
    pub domain_transition_ref_count: u32,
    pub guard_ref_count: u32,
    pub map_state_ref_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryExportArtifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub line_count: u32,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryRedaction {
    pub raw_chat_body_persisted: bool,
    pub raw_target_names_persisted: bool,
    pub compact_redacted: bool,
    pub all_log_refs_marked_redacted: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestTrajectoryEvidenceBoundary {
    pub manual_fsv_required_for_runtime: bool,
    pub is_training_script: bool,
    pub supports_contextgraph_export: bool,
    pub source_rows_verified_before_write: bool,
    pub note: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct TrajectoryHashPayload<'a> {
    schema_version: u32,
    profile_id: &'a str,
    trajectory_id: &'a str,
    intent: &'a EverQuestTrajectoryIntent,
    session_id: &'a str,
    transitions: &'a [EverQuestTrajectoryTransitionRow],
    source_refs: &'a [EverQuestTrajectorySourceRef],
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct TrajectoryExportLine<'a> {
    schema_version: u32,
    row_kind: &'a str,
    profile_id: &'a str,
    trajectory_id: &'a str,
    trajectory_hash: &'a str,
    intent: &'a EverQuestTrajectoryIntent,
    session_id: &'a str,
    summary: &'a EverQuestTrajectorySummary,
    transitions: &'a [EverQuestTrajectoryTransitionRow],
    source_refs: &'a [EverQuestTrajectorySourceRef],
    redaction: &'a EverQuestTrajectoryRedaction,
}

#[derive(Clone, Copy, Debug)]
struct TrajectoryExportRequest<'a> {
    profile_id: &'a str,
    trajectory_id: &'a str,
    trajectory_hash: &'a str,
    intent: &'a EverQuestTrajectoryIntent,
    session_id: &'a str,
    summary: &'a EverQuestTrajectorySummary,
    transitions: &'a [EverQuestTrajectoryTransitionRow],
    source_refs: &'a [EverQuestTrajectorySourceRef],
    redaction: &'a EverQuestTrajectoryRedaction,
}

#[derive(Clone, Debug)]
struct NormalizedTrajectoryParams {
    trajectory_id: String,
    profile_id: String,
    intent: EverQuestTrajectoryIntent,
    session_id: String,
    transitions: Vec<EverQuestTrajectoryTransitionInput>,
    source_refs: Vec<EverQuestTrajectorySourceRef>,
    export_jsonl: bool,
    row_key: String,
}

#[derive(Debug, Default)]
struct TransitionOrderState {
    seen_ids: HashSet<String>,
    previous_sequence: Option<u32>,
    previous_at: Option<DateTime<Utc>>,
}

impl TransitionOrderState {
    fn validate_next(
        &mut self,
        index: usize,
        transition_id: &str,
        sequence: u32,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), ErrorData> {
        if !self.seen_ids.insert(transition_id.to_owned()) {
            return Err(params_error(format!(
                "duplicate transition_id in trajectory: {transition_id}"
            )));
        }
        if self
            .previous_sequence
            .is_some_and(|previous| sequence <= previous)
        {
            return Err(params_error(format!(
                "transitions[{index}].sequence must be strictly increasing"
            )));
        }
        if self
            .previous_at
            .is_some_and(|previous| occurred_at < previous)
        {
            return Err(params_error(format!(
                "transitions[{index}].occurred_at is out of order"
            )));
        }
        self.previous_sequence = Some(sequence);
        self.previous_at = Some(occurred_at);
        Ok(())
    }
}

#[tool_router(router = everquest_trajectory_tool_router, vis = "pub(super)")]
impl SynapseService {
    #[tool(
        description = "Persist one ordered EverQuest trajectory from linked action, observation, event, log, state, and outcome evidence with JSONL provenance readback"
    )]
    pub async fn everquest_trajectory_record(
        &self,
        params: Parameters<EverQuestTrajectoryRecordParams>,
    ) -> Result<Json<EverQuestTrajectoryRecordResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = TOOL,
            "tool.invocation kind=everquest_trajectory_record"
        );
        let params = normalize_params(params.0)?;
        let response = self.record_trajectory(params)?;
        Ok(Json(response))
    }
}

impl SynapseService {
    fn record_trajectory(
        &self,
        params: NormalizedTrajectoryParams,
    ) -> Result<EverQuestTrajectoryRecordResponse, ErrorData> {
        if let Some(existing) = self.existing_trajectory_response(&params.row_key)? {
            return Ok(existing);
        }
        let row = self.build_trajectory_row(params)?;
        self.write_trajectory_row(&row)
    }

    fn existing_trajectory_response(
        &self,
        row_key: &str,
    ) -> Result<Option<EverQuestTrajectoryRecordResponse>, ErrorData> {
        let runtime = self.reflex_runtime()?;
        let runtime = runtime.lock().map_err(|_| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                "reflex runtime lock poisoned while recording EverQuest trajectory",
            )
        })?;

        let Some(existing) = runtime
            .storage_kv_row(row_key.as_bytes())
            .map_err(|error| mcp_error(error.code(), error.to_string()))?
        else {
            return Ok(None);
        };
        drop(runtime);
        let stored_value_len_bytes = len_to_u64(existing.len());
        let existing =
            decode_json_row::<EverQuestTrajectoryRow>(&existing, "EverQuest trajectory row")?;
        Ok(Some(EverQuestTrajectoryRecordResponse {
            ok: true,
            row_key: existing.row_key.clone(),
            duplicate_of_prior_row: true,
            stored_value_len_bytes,
            summary: existing.summary.clone(),
            trajectory: existing,
        }))
    }

    fn build_trajectory_row(
        &self,
        params: NormalizedTrajectoryParams,
    ) -> Result<EverQuestTrajectoryRow, ErrorData> {
        let runtime = self.reflex_runtime()?;
        let runtime = runtime.lock().map_err(|_| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                "reflex runtime lock poisoned while recording EverQuest trajectory",
            )
        })?;
        let transitions = linked_transition_rows(&runtime, params.transitions)?;
        let summary = trajectory_summary(&transitions)?;
        let redaction = redaction(&transitions);
        let trajectory_hash = trajectory_hash(
            &params.profile_id,
            &params.trajectory_id,
            &params.intent,
            &params.session_id,
            &transitions,
            &params.source_refs,
        )?;
        drop(runtime);

        let export = if params.export_jsonl {
            Some(write_jsonl_export(TrajectoryExportRequest {
                profile_id: &params.profile_id,
                trajectory_id: &params.trajectory_id,
                trajectory_hash: &trajectory_hash,
                intent: &params.intent,
                session_id: &params.session_id,
                summary: &summary,
                transitions: &transitions,
                source_refs: &params.source_refs,
                redaction: &redaction,
            })?)
        } else {
            None
        };

        Ok(EverQuestTrajectoryRow {
            schema_version: SCHEMA_VERSION,
            row_kind: "everquest_trajectory".to_owned(),
            profile_id: params.profile_id,
            trajectory_id: params.trajectory_id,
            row_key: params.row_key,
            intent: params.intent,
            session_id: params.session_id,
            created_at: Utc::now(),
            trajectory_hash,
            transitions,
            summary,
            source_refs: params.source_refs,
            export,
            redaction,
            evidence_boundary: evidence_boundary(),
        })
    }

    fn write_trajectory_row(
        &self,
        row: &EverQuestTrajectoryRow,
    ) -> Result<EverQuestTrajectoryRecordResponse, ErrorData> {
        let encoded = serde_json::to_vec(&row).map_err(|error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!("encode EverQuest trajectory row: {error}"),
            )
        })?;
        let runtime = self.reflex_runtime()?;
        let runtime = runtime.lock().map_err(|_| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                "reflex runtime lock poisoned while writing EverQuest trajectory",
            )
        })?;
        runtime
            .storage_put_kv_rows(vec![(row.row_key.as_bytes().to_vec(), encoded)])
            .map_err(|error| {
                mcp_error(
                    error_codes::STORAGE_WRITE_FAILED,
                    format!("write EverQuest trajectory row: {error}"),
                )
            })?;
        let stored = runtime
            .storage_kv_row(row.row_key.as_bytes())
            .map_err(|error| {
                mcp_error(
                    error_codes::STORAGE_READ_FAILED,
                    format!("read EverQuest trajectory row after write: {error}"),
                )
            })?
            .ok_or_else(|| {
                mcp_error(
                    error_codes::STORAGE_READ_FAILED,
                    format!(
                        "EverQuest trajectory row missing after write: {}",
                        row.row_key
                    ),
                )
            })?;
        drop(runtime);
        let readback = decode_json_row::<EverQuestTrajectoryRow>(&stored, "EverQuest trajectory")?;
        Ok(EverQuestTrajectoryRecordResponse {
            ok: true,
            row_key: readback.row_key.clone(),
            duplicate_of_prior_row: false,
            stored_value_len_bytes: len_to_u64(stored.len()),
            summary: readback.summary.clone(),
            trajectory: readback,
        })
    }
}

fn linked_transition_rows(
    runtime: &synapse_reflex::ReflexRuntime,
    transitions: Vec<EverQuestTrajectoryTransitionInput>,
) -> Result<Vec<EverQuestTrajectoryTransitionRow>, ErrorData> {
    transitions
        .into_iter()
        .enumerate()
        .map(|(index, transition)| linked_transition_row(runtime, index, transition))
        .collect()
}

fn linked_transition_row(
    runtime: &synapse_reflex::ReflexRuntime,
    index: usize,
    transition: EverQuestTrajectoryTransitionInput,
) -> Result<EverQuestTrajectoryTransitionRow, ErrorData> {
    let linked_state_len_bytes =
        read_required_kv_len(runtime, "state_row_key", &transition.state_row_key)?;
    let linked_domain_transition_len_bytes = transition
        .domain_transition_row_key
        .as_deref()
        .map(|key| read_required_kv_len(runtime, "domain_transition_row_key", key))
        .transpose()?;
    let linked_outcome_len_bytes = transition
        .outcome_row_key
        .as_deref()
        .map(|key| read_required_kv_len(runtime, "outcome_row_key", key))
        .transpose()?;
    let linked_guard_len_bytes = transition
        .guard_row_key
        .as_deref()
        .map(|key| read_required_kv_len(runtime, "guard_row_key", key))
        .transpose()?;
    let linked_map_state_len_bytes = transition
        .map_state_row_key
        .as_deref()
        .map(|key| read_required_kv_len(runtime, "map_state_row_key", key))
        .transpose()?;
    let action_refs = storage_refs(
        runtime,
        &format!("transitions[{index}].action_refs"),
        transition.action_refs,
        cf::CF_ACTION_LOG,
    )?;
    let observation_refs = storage_refs(
        runtime,
        &format!("transitions[{index}].observation_refs"),
        transition.observation_refs,
        cf::CF_OBSERVATIONS,
    )?;
    let event_refs = storage_refs(
        runtime,
        &format!("transitions[{index}].event_refs"),
        transition.event_refs,
        cf::CF_EVENTS,
    )?;
    let log_refs = transition
        .log_refs
        .into_iter()
        .enumerate()
        .map(|(ref_index, source)| {
            validate_log_ref(
                &format!("transitions[{index}].log_refs[{ref_index}]"),
                source,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EverQuestTrajectoryTransitionRow {
        transition_id: transition.transition_id,
        sequence: transition.sequence,
        occurred_at: transition.occurred_at,
        state_row_key: transition.state_row_key,
        domain_transition_row_key: transition.domain_transition_row_key,
        outcome_row_key: transition.outcome_row_key,
        guard_row_key: transition.guard_row_key,
        map_state_row_key: transition.map_state_row_key,
        linked_state_len_bytes,
        linked_domain_transition_len_bytes,
        linked_outcome_len_bytes,
        linked_guard_len_bytes,
        linked_map_state_len_bytes,
        action_refs,
        observation_refs,
        event_refs,
        log_refs,
        summary: transition.summary,
    })
}

fn storage_refs(
    runtime: &synapse_reflex::ReflexRuntime,
    field: &str,
    refs: Vec<EverQuestTrajectoryStorageRefInput>,
    required_cf: &str,
) -> Result<Vec<EverQuestTrajectoryStorageRef>, ErrorData> {
    if refs.is_empty() {
        return Err(params_error(format!(
            "{field} must contain at least one {required_cf} source row"
        )));
    }
    if refs.len() > MAX_REFS_PER_TRANSITION {
        return Err(params_error(format!(
            "{field} must contain <= {MAX_REFS_PER_TRANSITION} refs"
        )));
    }
    refs.into_iter()
        .enumerate()
        .map(|(index, source)| {
            let source = normalize_storage_ref(&format!("{field}[{index}]"), source, required_cf)?;
            let key_bytes = source_key_bytes(&source)?;
            let rows = runtime
                .storage_cf_prefix_rows(required_cf, &key_bytes, 1)
                .map_err(|error| mcp_error(error.code(), error.to_string()))?;
            let Some((row_key, value)) =
                rows.into_iter().find(|(row_key, _)| row_key == &key_bytes)
            else {
                return Err(mcp_error(
                    error_codes::STORAGE_READ_FAILED,
                    format!("{field}[{index}] source row not found in {required_cf}"),
                ));
            };
            Ok(EverQuestTrajectoryStorageRef {
                cf_name: source.cf_name,
                row_key: source.row_key,
                row_key_hex: hex_encode(&row_key),
                value_len_bytes: len_to_u64(value.len()),
                summary: source.summary,
            })
        })
        .collect()
}

fn read_required_kv_len(
    runtime: &synapse_reflex::ReflexRuntime,
    field: &str,
    row_key: &str,
) -> Result<u64, ErrorData> {
    let row_key = normalize_required_text(field, row_key)?;
    let stored = runtime
        .storage_kv_row(row_key.as_bytes())
        .map_err(|error| mcp_error(error.code(), error.to_string()))?
        .ok_or_else(|| {
            mcp_error(
                error_codes::STORAGE_READ_FAILED,
                format!("{field} source row not found in CF_KV: {row_key}"),
            )
        })?;
    Ok(len_to_u64(stored.len()))
}

fn normalize_params(
    params: EverQuestTrajectoryRecordParams,
) -> Result<NormalizedTrajectoryParams, ErrorData> {
    let profile_id = validate_profile_id(&params.profile_id)?;
    let trajectory_id = validate_id("trajectory_id", &params.trajectory_id)?;
    let session_id = validate_id("session_id", &params.session_id)?;
    if params.transitions.is_empty() {
        return Err(params_error(
            "transitions must contain at least one ordered transition",
        ));
    }
    if params.transitions.len() > MAX_TRANSITIONS {
        return Err(params_error(format!(
            "transitions must contain <= {MAX_TRANSITIONS} rows"
        )));
    }
    if params.source_refs.is_empty() {
        return Err(params_error(
            "source_refs must contain at least one trajectory provenance ref",
        ));
    }
    if params.source_refs.len() > MAX_SOURCE_REFS {
        return Err(params_error(format!(
            "source_refs must contain <= {MAX_SOURCE_REFS} refs"
        )));
    }
    let source_refs = params
        .source_refs
        .into_iter()
        .enumerate()
        .map(|(index, source)| normalize_source_ref(&format!("source_refs[{index}]"), source))
        .collect::<Result<Vec<_>, _>>()?;
    let transitions = normalize_transition_inputs(params.transitions)?;
    let row_key = trajectory_row_key(&profile_id, &trajectory_id);
    Ok(NormalizedTrajectoryParams {
        trajectory_id,
        profile_id,
        intent: params.intent,
        session_id,
        transitions,
        source_refs,
        export_jsonl: params.export_jsonl,
        row_key,
    })
}

fn normalize_transition_inputs(
    transitions: Vec<EverQuestTrajectoryTransitionInput>,
) -> Result<Vec<EverQuestTrajectoryTransitionInput>, ErrorData> {
    let mut order = TransitionOrderState::default();
    transitions
        .into_iter()
        .enumerate()
        .map(|(index, transition)| normalize_transition_input(index, transition, &mut order))
        .collect()
}

fn normalize_transition_input(
    index: usize,
    transition: EverQuestTrajectoryTransitionInput,
    order: &mut TransitionOrderState,
) -> Result<EverQuestTrajectoryTransitionInput, ErrorData> {
    let field = format!("transitions[{index}]");
    let transition_id = validate_id(&format!("{field}.transition_id"), &transition.transition_id)?;
    order.validate_next(
        index,
        &transition_id,
        transition.sequence,
        transition.occurred_at,
    )?;
    require_nonempty_refs(&field, &transition)?;
    Ok(EverQuestTrajectoryTransitionInput {
        transition_id,
        sequence: transition.sequence,
        occurred_at: transition.occurred_at,
        state_row_key: normalize_required_text(
            &format!("{field}.state_row_key"),
            &transition.state_row_key,
        )?,
        domain_transition_row_key: normalize_optional_text(
            &format!("{field}.domain_transition_row_key"),
            transition.domain_transition_row_key,
        )?,
        outcome_row_key: normalize_optional_text(
            &format!("{field}.outcome_row_key"),
            transition.outcome_row_key,
        )?,
        guard_row_key: normalize_optional_text(
            &format!("{field}.guard_row_key"),
            transition.guard_row_key,
        )?,
        map_state_row_key: normalize_optional_text(
            &format!("{field}.map_state_row_key"),
            transition.map_state_row_key,
        )?,
        action_refs: transition.action_refs,
        observation_refs: transition.observation_refs,
        event_refs: transition.event_refs,
        log_refs: normalize_log_refs(&field, transition.log_refs)?,
        summary: normalize_optional_text(&format!("{field}.summary"), transition.summary)?,
    })
}

fn require_nonempty_refs(
    field: &str,
    transition: &EverQuestTrajectoryTransitionInput,
) -> Result<(), ErrorData> {
    if transition.action_refs.is_empty() {
        return Err(params_error(format!(
            "{field}.action_refs must not be empty"
        )));
    }
    if transition.observation_refs.is_empty() {
        return Err(params_error(format!(
            "{field}.observation_refs must not be empty"
        )));
    }
    if transition.event_refs.is_empty() {
        return Err(params_error(format!(
            "{field}.event_refs must not be empty"
        )));
    }
    if transition.log_refs.is_empty() {
        return Err(params_error(format!("{field}.log_refs must not be empty")));
    }
    Ok(())
}

fn normalize_log_refs(
    field: &str,
    refs: Vec<EverQuestTrajectoryLogRef>,
) -> Result<Vec<EverQuestTrajectoryLogRef>, ErrorData> {
    refs.into_iter()
        .enumerate()
        .map(|(ref_index, source)| {
            normalize_log_ref(&format!("{field}.log_refs[{ref_index}]"), source)
        })
        .collect()
}

fn normalize_optional_text(
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, ErrorData> {
    value
        .map(|value| normalize_required_text(field, &value))
        .transpose()
}

fn normalize_storage_ref(
    field: &str,
    source: EverQuestTrajectoryStorageRefInput,
    required_cf: &str,
) -> Result<EverQuestTrajectoryStorageRefInput, ErrorData> {
    let cf_name = normalize_required_text(&format!("{field}.cf_name"), &source.cf_name)?;
    if cf_name != required_cf {
        return Err(params_error(format!(
            "{field}.cf_name must be {required_cf:?}; got {cf_name:?}"
        )));
    }
    if source.row_key.is_none() && source.row_key_hex.is_none() {
        return Err(params_error(format!(
            "{field} requires row_key or row_key_hex"
        )));
    }
    Ok(EverQuestTrajectoryStorageRefInput {
        cf_name,
        row_key: source
            .row_key
            .map(|value| normalize_required_text(&format!("{field}.row_key"), &value))
            .transpose()?,
        row_key_hex: source
            .row_key_hex
            .map(|value| validate_hex_key(&format!("{field}.row_key_hex"), &value))
            .transpose()?,
        summary: source
            .summary
            .map(|value| normalize_required_text(&format!("{field}.summary"), &value))
            .transpose()?,
    })
}

fn normalize_source_ref(
    field: &str,
    source: EverQuestTrajectorySourceRef,
) -> Result<EverQuestTrajectorySourceRef, ErrorData> {
    Ok(EverQuestTrajectorySourceRef {
        kind: normalize_required_text(&format!("{field}.kind"), &source.kind)?,
        row_key: source
            .row_key
            .map(|value| normalize_required_text(&format!("{field}.row_key"), &value))
            .transpose()?,
        path: source
            .path
            .map(|value| normalize_required_text(&format!("{field}.path"), &value))
            .transpose()?,
        start_offset: source.start_offset,
        next_offset: source.next_offset,
        content_sha256: source
            .content_sha256
            .map(|value| validate_sha256(&format!("{field}.content_sha256"), &value))
            .transpose()?,
        summary: source
            .summary
            .map(|value| normalize_required_text(&format!("{field}.summary"), &value))
            .transpose()?,
    })
}

fn normalize_log_ref(
    field: &str,
    source: EverQuestTrajectoryLogRef,
) -> Result<EverQuestTrajectoryLogRef, ErrorData> {
    let path = normalize_required_text(&format!("{field}.path"), &source.path)?;
    if source.next_offset <= source.start_offset {
        return Err(params_error(format!(
            "{field}.next_offset must be greater than start_offset"
        )));
    }
    Ok(EverQuestTrajectoryLogRef {
        path,
        start_offset: source.start_offset,
        next_offset: source.next_offset,
        event_kind: normalize_required_text(&format!("{field}.event_kind"), &source.event_kind)?,
        content_sha256: source
            .content_sha256
            .map(|value| validate_sha256(&format!("{field}.content_sha256"), &value))
            .transpose()?,
        redacted: source.redacted,
        summary: source
            .summary
            .map(|value| normalize_required_text(&format!("{field}.summary"), &value))
            .transpose()?,
    })
}

fn validate_log_ref(
    field: &str,
    source: EverQuestTrajectoryLogRef,
) -> Result<EverQuestTrajectoryLogRef, ErrorData> {
    let path = PathBuf::from(&source.path);
    let metadata = fs::metadata(&path).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_READ_FAILED,
            format!("{field} metadata read failed: {error}"),
        )
    })?;
    if !metadata.is_file() {
        return Err(mcp_error(
            error_codes::STORAGE_READ_FAILED,
            format!("{field} path is not a file"),
        ));
    }
    if source.next_offset > metadata.len() {
        return Err(params_error(format!(
            "{field}.next_offset exceeds file length {}",
            metadata.len()
        )));
    }
    if let Some(expected_hash) = &source.content_sha256 {
        let len = usize::try_from(source.next_offset.saturating_sub(source.start_offset))
            .map_err(|_| params_error(format!("{field} byte range is too large")))?;
        let mut file = fs::File::open(&path).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_READ_FAILED,
                format!("{field} open failed: {error}"),
            )
        })?;
        file.seek(SeekFrom::Start(source.start_offset))
            .map_err(|error| {
                mcp_error(
                    error_codes::STORAGE_READ_FAILED,
                    format!("{field} seek failed: {error}"),
                )
            })?;
        let mut bytes = vec![0_u8; len];
        file.read_exact(&mut bytes).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_READ_FAILED,
                format!("{field} read failed: {error}"),
            )
        })?;
        let actual_hash = sha256_hex(&bytes);
        if &actual_hash != expected_hash {
            return Err(params_error(format!(
                "{field}.content_sha256 does not match physical log bytes"
            )));
        }
    }
    Ok(source)
}

fn source_key_bytes(source: &EverQuestTrajectoryStorageRefInput) -> Result<Vec<u8>, ErrorData> {
    source.row_key_hex.as_deref().map_or_else(
        || {
            source.row_key.as_ref().map_or_else(
                || Err(params_error("source ref missing row key")),
                |row_key| Ok(row_key.as_bytes().to_vec()),
            )
        },
        decode_hex,
    )
}

fn trajectory_summary(
    transitions: &[EverQuestTrajectoryTransitionRow],
) -> Result<EverQuestTrajectorySummary, ErrorData> {
    let Some(first) = transitions.first() else {
        return Err(params_error("transitions must not be empty"));
    };
    let last = transitions.last().unwrap_or(first);
    Ok(EverQuestTrajectorySummary {
        transition_count: len_to_u32(transitions.len()),
        first_at: first.occurred_at,
        last_at: last.occurred_at,
        action_ref_count: count_refs(transitions, |transition| transition.action_refs.len()),
        observation_ref_count: count_refs(transitions, |transition| {
            transition.observation_refs.len()
        }),
        event_ref_count: count_refs(transitions, |transition| transition.event_refs.len()),
        log_ref_count: count_refs(transitions, |transition| transition.log_refs.len()),
        state_ref_count: len_to_u32(transitions.len()),
        domain_transition_ref_count: count_refs(transitions, |transition| {
            usize::from(transition.domain_transition_row_key.is_some())
        }),
        guard_ref_count: count_refs(transitions, |transition| {
            usize::from(transition.guard_row_key.is_some())
        }),
        map_state_ref_count: count_refs(transitions, |transition| {
            usize::from(transition.map_state_row_key.is_some())
        }),
    })
}

fn count_refs<F>(transitions: &[EverQuestTrajectoryTransitionRow], count: F) -> u32
where
    F: Fn(&EverQuestTrajectoryTransitionRow) -> usize,
{
    len_to_u32(transitions.iter().map(count).sum())
}

fn trajectory_hash(
    profile_id: &str,
    trajectory_id: &str,
    intent: &EverQuestTrajectoryIntent,
    session_id: &str,
    transitions: &[EverQuestTrajectoryTransitionRow],
    source_refs: &[EverQuestTrajectorySourceRef],
) -> Result<String, ErrorData> {
    let payload = TrajectoryHashPayload {
        schema_version: SCHEMA_VERSION,
        profile_id,
        trajectory_id,
        intent,
        session_id,
        transitions,
        source_refs,
    };
    let encoded = serde_json::to_vec(&payload).map_err(|error| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            format!("encode EverQuest trajectory hash payload: {error}"),
        )
    })?;
    Ok(sha256_hex(&encoded))
}

fn write_jsonl_export(
    request: TrajectoryExportRequest<'_>,
) -> Result<EverQuestTrajectoryExportArtifact, ErrorData> {
    let root = trajectory_export_root();
    let dir = root.join(request.profile_id);
    fs::create_dir_all(&dir).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_WRITE_FAILED,
            format!("create EverQuest trajectory export dir: {error}"),
        )
    })?;
    let file_name = format!("{}.jsonl", request.trajectory_id);
    let final_path = dir.join(file_name);
    let temp_path = dir.join(format!("{}.tmp", request.trajectory_id));
    let line = TrajectoryExportLine {
        schema_version: SCHEMA_VERSION,
        row_kind: "everquest_trajectory_export",
        profile_id: request.profile_id,
        trajectory_id: request.trajectory_id,
        trajectory_hash: request.trajectory_hash,
        intent: request.intent,
        session_id: request.session_id,
        summary: request.summary,
        transitions: request.transitions,
        source_refs: request.source_refs,
        redaction: request.redaction,
    };
    let mut encoded = serde_json::to_vec(&line).map_err(|error| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            format!("encode EverQuest trajectory JSONL: {error}"),
        )
    })?;
    encoded.push(b'\n');
    fs::write(&temp_path, &encoded).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_WRITE_FAILED,
            format!("write EverQuest trajectory temp JSONL: {error}"),
        )
    })?;
    let temp_readback = fs::read(&temp_path).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_READ_FAILED,
            format!("read EverQuest trajectory temp JSONL: {error}"),
        )
    })?;
    if temp_readback != encoded {
        return Err(mcp_error(
            error_codes::STORAGE_CORRUPTED,
            "EverQuest trajectory temp JSONL readback mismatch",
        ));
    }
    if final_path.exists() {
        fs::remove_file(&final_path).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_WRITE_FAILED,
                format!("replace EverQuest trajectory JSONL: {error}"),
            )
        })?;
    }
    fs::rename(&temp_path, &final_path).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_WRITE_FAILED,
            format!("promote EverQuest trajectory JSONL: {error}"),
        )
    })?;
    let final_readback = fs::read(&final_path).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_READ_FAILED,
            format!("read EverQuest trajectory JSONL: {error}"),
        )
    })?;
    Ok(EverQuestTrajectoryExportArtifact {
        path: final_path.display().to_string(),
        bytes: len_to_u64(final_readback.len()),
        sha256: sha256_hex(&final_readback),
        line_count: 1,
    })
}

fn redaction(transitions: &[EverQuestTrajectoryTransitionRow]) -> EverQuestTrajectoryRedaction {
    let all_log_refs_marked_redacted = transitions
        .iter()
        .flat_map(|transition| &transition.log_refs)
        .all(|source| source.redacted);
    EverQuestTrajectoryRedaction {
        raw_chat_body_persisted: false,
        raw_target_names_persisted: false,
        compact_redacted: true,
        all_log_refs_marked_redacted,
    }
}

fn evidence_boundary() -> EverQuestTrajectoryEvidenceBoundary {
    EverQuestTrajectoryEvidenceBoundary {
        manual_fsv_required_for_runtime: true,
        is_training_script: false,
        supports_contextgraph_export: true,
        source_rows_verified_before_write: true,
        note: "Trajectory rows link compact source refs and JSONL provenance; manual FSV still requires physical source-of-truth readback for gameplay behavior."
            .to_owned(),
    }
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

fn validate_hex_key(field: &str, value: &str) -> Result<String, ErrorData> {
    let value = normalize_required_text(field, value)?;
    let _ = decode_hex(&value)?;
    Ok(value.to_ascii_lowercase())
}

fn validate_sha256(field: &str, value: &str) -> Result<String, ErrorData> {
    let value = normalize_required_text(field, value)?;
    if value.len() != HASH_BYTES * 2 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(params_error(format!(
            "{field} must be a lowercase/uppercase SHA-256 hex digest"
        )));
    }
    Ok(value.to_ascii_lowercase())
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ErrorData> {
    let value = value.trim();
    if !value.len().is_multiple_of(2) || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(params_error(
            "hex key must contain an even number of hex digits",
        ));
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();
    for chunk in bytes.chunks_exact(2) {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| params_error("hex key contains invalid UTF-8"))?;
        let byte = u8::from_str_radix(text, 16)
            .map_err(|_| params_error("hex key contains invalid byte"))?;
        out.push(byte);
    }
    Ok(out)
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

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

fn decode_json_row<T>(bytes: &[u8], label: &str) -> Result<T, ErrorData>
where
    T: DeserializeOwned,
{
    serde_json::from_slice::<T>(bytes).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_CORRUPTED,
            format!("decode {label}: {error}"),
        )
    })
}

fn trajectory_row_key(profile_id: &str, trajectory_id: &str) -> String {
    format!("{TRAJECTORY_ROW_PREFIX}/{profile_id}/{trajectory_id}")
}

fn trajectory_export_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("synapse")
        .join("everquest")
        .join("trajectories")
}

fn params_error(message: impl Into<String>) -> ErrorData {
    mcp_error(error_codes::TOOL_PARAMS_INVALID, message)
}

fn default_profile_id() -> String {
    EVERQUEST_PROFILE_ID.to_owned()
}

const fn default_export_jsonl() -> bool {
    true
}

fn len_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn len_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_transition_ids() {
        let mut params = base_params();
        params.transitions.push(params.transitions[0].clone());
        params.transitions[1].sequence = 2;
        params.transitions[1].occurred_at = "2026-05-29T12:01:00Z".parse().unwrap();
        let error = normalize_params(params).unwrap_err();
        assert_eq!(
            error.data.as_ref().and_then(|data| data.get("code")),
            Some(&serde_json::json!(error_codes::TOOL_PARAMS_INVALID))
        );
    }

    #[test]
    fn rejects_out_of_order_timestamps() {
        let mut params = base_params();
        let mut second = params.transitions[0].clone();
        second.transition_id = "transition-two".to_owned();
        second.sequence = 2;
        second.occurred_at = "2026-05-29T11:59:00Z".parse().unwrap();
        params.transitions.push(second);
        let error = normalize_params(params).unwrap_err();
        assert_eq!(
            error.data.as_ref().and_then(|data| data.get("code")),
            Some(&serde_json::json!(error_codes::TOOL_PARAMS_INVALID))
        );
    }

    #[test]
    fn trajectory_hash_changes_with_linked_source() {
        let params = normalize_params(base_params()).unwrap();
        let transitions = vec![transition_row("abc")];
        let hash_a = trajectory_hash(
            &params.profile_id,
            &params.trajectory_id,
            &params.intent,
            &params.session_id,
            &transitions,
            &params.source_refs,
        )
        .unwrap();
        let transitions = vec![transition_row("def")];
        let hash_b = trajectory_hash(
            &params.profile_id,
            &params.trajectory_id,
            &params.intent,
            &params.session_id,
            &transitions,
            &params.source_refs,
        )
        .unwrap();
        assert_ne!(hash_a, hash_b);
    }

    fn base_params() -> EverQuestTrajectoryRecordParams {
        EverQuestTrajectoryRecordParams {
            trajectory_id: "issue512-trajectory".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            intent: EverQuestTrajectoryIntent::NavigationProbe,
            session_id: "issue512-session".to_owned(),
            transitions: vec![EverQuestTrajectoryTransitionInput {
                transition_id: "transition-one".to_owned(),
                sequence: 1,
                occurred_at: "2026-05-29T12:00:00Z".parse().unwrap(),
                state_row_key: "everquest/current_state/v1/everquest.live".to_owned(),
                domain_transition_row_key: Some(
                    "everquest/dynamicjepa_transition/v1/everquest.live/transition-one".to_owned(),
                ),
                outcome_row_key: Some(
                    "everquest/outcome_event/v1/everquest.live/outcome-one".to_owned(),
                ),
                guard_row_key: None,
                map_state_row_key: None,
                action_refs: vec![storage_ref(cf::CF_ACTION_LOG, "0000000000000001")],
                observation_refs: vec![storage_ref(cf::CF_OBSERVATIONS, "0000000000000002")],
                event_refs: vec![storage_ref(cf::CF_EVENTS, "0000000000000003")],
                log_refs: vec![EverQuestTrajectoryLogRef {
                    path: "C:\\eq\\Logs\\eqlog_character_server.txt".to_owned(),
                    start_offset: 1,
                    next_offset: 2,
                    event_kind: "loc".to_owned(),
                    content_sha256: None,
                    redacted: true,
                    summary: Some("redacted loc event".to_owned()),
                }],
                summary: Some("one transition".to_owned()),
            }],
            source_refs: vec![EverQuestTrajectorySourceRef {
                kind: "manual_test".to_owned(),
                row_key: None,
                path: None,
                start_offset: None,
                next_offset: None,
                content_sha256: None,
                summary: Some("test source".to_owned()),
            }],
            export_jsonl: true,
        }
    }

    fn storage_ref(cf_name: &str, row_key_hex: &str) -> EverQuestTrajectoryStorageRefInput {
        EverQuestTrajectoryStorageRefInput {
            cf_name: cf_name.to_owned(),
            row_key: None,
            row_key_hex: Some(row_key_hex.to_owned()),
            summary: Some("test source row".to_owned()),
        }
    }

    fn transition_row(row_key_hex: &str) -> EverQuestTrajectoryTransitionRow {
        EverQuestTrajectoryTransitionRow {
            transition_id: "transition-one".to_owned(),
            sequence: 1,
            occurred_at: "2026-05-29T12:00:00Z".parse().unwrap(),
            state_row_key: "everquest/current_state/v1/everquest.live".to_owned(),
            domain_transition_row_key: None,
            outcome_row_key: None,
            guard_row_key: None,
            map_state_row_key: None,
            linked_state_len_bytes: 1,
            linked_domain_transition_len_bytes: None,
            linked_outcome_len_bytes: None,
            linked_guard_len_bytes: None,
            linked_map_state_len_bytes: None,
            action_refs: vec![EverQuestTrajectoryStorageRef {
                cf_name: cf::CF_ACTION_LOG.to_owned(),
                row_key: None,
                row_key_hex: row_key_hex.to_owned(),
                value_len_bytes: 1,
                summary: None,
            }],
            observation_refs: Vec::new(),
            event_refs: Vec::new(),
            log_refs: Vec::new(),
            summary: None,
        }
    }
}
