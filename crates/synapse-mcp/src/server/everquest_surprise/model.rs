use chrono::{DateTime, Utc};
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{
    everquest_log::EVERQUEST_PROFILE_ID,
    everquest_state::CURRENT_STATE_ROW_KEY,
    everquest_world_model::model::{
        EverQuestWorldModelRecordResponse, EverQuestWorldModelSourceRef,
    },
};

pub(super) const TOOL: &str = "everquest_surprise_detect";
pub(super) const SCHEMA_VERSION: u32 = 1;
pub(super) const DEFAULT_THRESHOLD: f32 = 0.50;
pub(super) const DEFAULT_STALE_AFTER_SECONDS: u64 = 300;
pub(super) const MAX_TEXT_BYTES: usize = 512;
pub(super) const MAX_SOURCE_REFS: usize = 32;
pub(super) const WORLD_MODEL_MAX_PAYLOAD_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestSurpriseDetectParams {
    pub surprise_id: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction: Option<EverQuestSurprisePrediction>,
    #[serde(default = "default_state_row_key")]
    pub observed_state_row_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_override: Option<EverQuestSurpriseObservedOverride>,
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    #[serde(default = "default_stale_after_seconds")]
    pub stale_after_seconds: u64,
    #[serde(default)]
    pub source_refs: Vec<EverQuestWorldModelSourceRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestSurprisePrediction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_outcome_kind: Option<String>,
    #[serde(default = "default_full_confidence")]
    pub confidence: f32,
    #[serde(default)]
    pub source_refs: Vec<EverQuestWorldModelSourceRef>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestSurpriseObservedOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_outcome_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_outcome_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<DateTime<Utc>>,
    #[serde(default = "default_full_confidence")]
    pub zone_confidence: f32,
    #[serde(default = "default_full_confidence")]
    pub outcome_confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_mode: Option<String>,
    #[serde(default)]
    pub source_refs: Vec<EverQuestWorldModelSourceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestSurpriseDetectResponse {
    pub ok: bool,
    pub row_key: String,
    pub stored_value_len_bytes: u64,
    pub decision: String,
    pub surprise_detected: bool,
    pub stop_condition: bool,
    pub world_model: EverQuestWorldModelRecordResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct EverQuestSurprisePayload {
    pub(super) schema_version: u32,
    pub(super) row_kind: String,
    pub(super) surprise_id: String,
    pub(super) detected_at: DateTime<Utc>,
    pub(super) prediction: Option<EverQuestSurprisePrediction>,
    pub(super) observed: EverQuestSurpriseObserved,
    pub(super) comparison: EverQuestSurpriseComparison,
    pub(super) remediation: Vec<String>,
    pub(super) evidence_boundary: EverQuestSurpriseEvidenceBoundary,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct EverQuestSurpriseObserved {
    pub(super) source_mode: String,
    pub(super) observed_state_row_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) observed_outcome_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) observed_zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) observed_outcome_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) observed_at: Option<DateTime<Utc>>,
    pub(super) zone_confidence: f32,
    pub(super) outcome_confidence: f32,
    pub(super) source_refs: Vec<EverQuestWorldModelSourceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct EverQuestSurpriseComparison {
    pub(super) decision: String,
    pub(super) surprise_detected: bool,
    pub(super) stop_condition: bool,
    pub(super) divergence_score: f32,
    pub(super) threshold: f32,
    pub(super) compared_fields: Vec<String>,
    pub(super) mismatch_reasons: Vec<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct EverQuestSurpriseEvidenceBoundary {
    pub(super) writes_world_model_row_only: bool,
    pub(super) executes_input: bool,
    pub(super) compact_redacted: bool,
    pub(super) manual_fsv_required_for_runtime: bool,
    pub(super) is_fsv_script: bool,
    pub(super) note: String,
}

fn default_profile_id() -> String {
    EVERQUEST_PROFILE_ID.to_owned()
}

fn default_state_row_key() -> String {
    CURRENT_STATE_ROW_KEY.to_owned()
}

const fn default_threshold() -> f32 {
    DEFAULT_THRESHOLD
}

const fn default_stale_after_seconds() -> u64 {
    DEFAULT_STALE_AFTER_SECONDS
}

const fn default_full_confidence() -> f32 {
    1.0
}
