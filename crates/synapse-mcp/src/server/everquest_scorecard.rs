use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use rmcp::{ErrorData, schemars::JsonSchema};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use synapse_core::error_codes;

use super::{
    Json, Parameters, SynapseService, everquest_log::EVERQUEST_PROFILE_ID, tool, tool_router,
};
use crate::m1::mcp_error;

const RECORD_TOOL: &str = "everquest_action_prior_record";
const SCORECARD_TOOL: &str = "everquest_action_prior_scorecard";
const SCHEMA_VERSION: u32 = 1;
const EVAL_ROW_PREFIX: &str = "everquest/action_prior_eval/v1";
const SCORECARD_ROW_PREFIX: &str = "everquest/action_prior_scorecard/v1";
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 512;
const MAX_TOP3_ACTIONS: usize = 3;
const MAX_SOURCE_EPISODE_IDS: usize = 64;
const MAX_SOURCE_REFS: usize = 32;
const MAX_LIMITATIONS: usize = 32;
const MAX_SCORECARD_SAMPLES: usize = 512;
const DEFAULT_MIN_SAMPLES: u32 = 3;
const DEFAULT_MIN_CONFIDENCE_FOR_ACTION: f32 = 0.60;
const DEFAULT_COMPETENCE_FLOOR: f32 = 0.60;
const DEFAULT_STRETCH_TARGET: f32 = 0.80;
const OVERCONFIDENT_WRONG_THRESHOLD: f32 = 0.80;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorRecordParams {
    pub sample_id: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    pub prediction_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_outcome_id: Option<String>,
    pub prediction: EverQuestActionPriorPrediction,
    pub actual: EverQuestActionPriorActual,
    #[serde(default)]
    pub source_episode_ids: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<EverQuestActionPriorSourceRef>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorScorecardParams {
    pub window_id: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    #[serde(default)]
    pub sample_ids: Vec<String>,
    #[serde(default = "default_min_samples")]
    pub min_samples: u32,
    #[serde(default = "default_min_confidence_for_action")]
    pub min_confidence_for_action: f32,
    #[serde(default = "default_competence_floor")]
    pub competence_floor: f32,
    #[serde(default = "default_stretch_target")]
    pub stretch_target: f32,
    #[serde(default)]
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorRecordResponse {
    pub ok: bool,
    pub row_key: String,
    pub stored_value_len_bytes: u64,
    pub sample: EverQuestActionPriorEvalRow,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorScorecardResponse {
    pub ok: bool,
    pub row_key: String,
    pub stored_value_len_bytes: u64,
    pub scorecard: EverQuestActionPriorScorecardRow,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorEvalRow {
    pub schema_version: u32,
    pub row_kind: String,
    pub profile_id: String,
    pub sample_id: String,
    pub prediction_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_outcome_id: Option<String>,
    pub recorded_at: DateTime<Utc>,
    pub prediction: EverQuestActionPriorPrediction,
    pub actual: EverQuestActionPriorActual,
    pub correctness: EverQuestActionPriorCorrectness,
    pub source_episode_ids: Vec<String>,
    pub source_refs: Vec<EverQuestActionPriorSourceRef>,
    pub limitations: Vec<String>,
    pub evidence_boundary: EverQuestActionPriorEvidenceBoundary,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorScorecardRow {
    pub schema_version: u32,
    pub row_kind: String,
    pub profile_id: String,
    pub window_id: String,
    pub generated_at: DateTime<Utc>,
    pub window_bounds: EverQuestActionPriorWindowBounds,
    pub source_episode_ids: Vec<String>,
    pub source_sample_keys: Vec<String>,
    pub metrics: EverQuestActionPriorMetrics,
    pub calibration_buckets: Vec<EverQuestActionPriorCalibrationBucket>,
    pub competence: EverQuestActionPriorCompetence,
    pub limitations: Vec<String>,
    pub evidence_boundary: EverQuestActionPriorEvidenceBoundary,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorWindowBounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_sample_recorded_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sample_recorded_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorPrediction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(default)]
    pub top3_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coord_bucket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hazard_avoidance: Option<bool>,
    pub confidence: f32,
    #[serde(default)]
    pub abstain: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorActual {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coord_bucket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hazard_occurred: Option<bool>,
    #[serde(default)]
    pub surprise: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorSourceRef {
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
    pub note: Option<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorCorrectness {
    pub class: String,
    pub abstained: bool,
    pub actual_known: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top1_correct: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top3_correct: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_correct: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coord_bucket_correct: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hazard_avoidance_correct: Option<bool>,
    pub useful: bool,
    pub confidence_bucket: String,
    pub overconfident_wrong: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorMetrics {
    pub sample_count: u32,
    pub min_samples: u32,
    pub evaluated_count: u32,
    pub abstention_count: u32,
    pub honest_abstention_count: u32,
    pub unknown_actual_count: u32,
    pub low_confidence_action_count: u32,
    pub overconfident_wrong_count: u32,
    pub surprise_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surprise_rate: Option<f32>,
    pub top1_total: u32,
    pub top1_correct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top1_accuracy: Option<f32>,
    pub top3_total: u32,
    pub top3_correct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top3_accuracy: Option<f32>,
    pub zone_total: u32,
    pub zone_correct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_accuracy: Option<f32>,
    pub coord_bucket_total: u32,
    pub coord_bucket_correct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coord_bucket_accuracy: Option<f32>,
    pub hazard_avoidance_total: u32,
    pub hazard_avoidance_correct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hazard_avoidance_accuracy: Option<f32>,
    pub useful_total: u32,
    pub useful_correct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub useful_accuracy: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervised_utility_rate: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorCalibrationBucket {
    pub bucket: String,
    pub lower_inclusive: f32,
    pub upper_exclusive: Option<f32>,
    pub sample_count: u32,
    pub evaluated_count: u32,
    pub useful_correct: u32,
    pub abstention_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub useful_accuracy: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorCompetence {
    pub minimum_floor: f32,
    pub stretch_target: f32,
    pub status: String,
    pub meets_minimum_floor: bool,
    pub meets_stretch_target: bool,
    pub minimum_is_floor_not_ceiling: bool,
    pub optimization_target: String,
    pub recommendation: String,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestActionPriorEvidenceBoundary {
    pub supports_planning_quality: bool,
    pub manual_fsv_required_for_runtime: bool,
    pub is_fsv: bool,
    pub redacted: bool,
    pub note: String,
}

#[tool_router(router = everquest_scorecard_tool_router, vis = "pub(super)")]
impl SynapseService {
    #[tool(
        description = "Persist one EverQuest action-prior prediction/outcome sample with computed correctness and exact CF_KV readback"
    )]
    pub async fn everquest_action_prior_record(
        &self,
        params: Parameters<EverQuestActionPriorRecordParams>,
    ) -> Result<Json<EverQuestActionPriorRecordResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = RECORD_TOOL,
            "tool.invocation kind=everquest_action_prior_record"
        );
        let row = eval_row_from_params(params.0)?;
        let key = eval_row_key(&row.profile_id, &row.sample_id);
        let (sample, stored_value_len_bytes) =
            self.persist_kv_json(&key, &row, "EverQuest action-prior eval row")?;
        Ok(Json(EverQuestActionPriorRecordResponse {
            ok: true,
            row_key: key,
            stored_value_len_bytes,
            sample,
        }))
    }

    #[tool(
        description = "Aggregate persisted EverQuest action-prior samples into a floor-not-ceiling competence scorecard with exact CF_KV readback"
    )]
    pub async fn everquest_action_prior_scorecard(
        &self,
        params: Parameters<EverQuestActionPriorScorecardParams>,
    ) -> Result<Json<EverQuestActionPriorScorecardResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = SCORECARD_TOOL,
            "tool.invocation kind=everquest_action_prior_scorecard"
        );
        let params = normalize_scorecard_params(params.0)?;
        let samples = self.read_eval_rows(&params.profile_id, &params.sample_ids)?;
        let row = scorecard_row_from_samples(&params, &samples);
        let key = scorecard_row_key(&row.profile_id, &row.window_id);
        let (scorecard, stored_value_len_bytes) =
            self.persist_kv_json(&key, &row, "EverQuest action-prior scorecard row")?;
        Ok(Json(EverQuestActionPriorScorecardResponse {
            ok: true,
            row_key: key,
            stored_value_len_bytes,
            scorecard,
        }))
    }
}

impl SynapseService {
    fn read_eval_rows(
        &self,
        profile_id: &str,
        sample_ids: &[String],
    ) -> Result<Vec<EverQuestActionPriorEvalRow>, ErrorData> {
        let rows = {
            let runtime = self.reflex_runtime()?;
            let runtime = runtime.lock().map_err(|_error| {
                mcp_error(
                    error_codes::TOOL_INTERNAL_ERROR,
                    "reflex runtime lock poisoned while reading EverQuest action-prior eval rows",
                )
            })?;
            sample_ids
                .iter()
                .map(|sample_id| {
                    let key = eval_row_key(profile_id, sample_id);
                    let value = runtime
                        .storage_kv_row(key.as_bytes())
                        .map_err(|error| mcp_error(error.code(), error.to_string()))?
                        .ok_or_else(|| {
                            mcp_error(
                                error_codes::STORAGE_READ_FAILED,
                                format!("EverQuest action-prior eval row missing: {key}"),
                            )
                        })?;
                    let row = serde_json::from_slice::<EverQuestActionPriorEvalRow>(&value)
                        .map_err(|error| {
                            mcp_error(
                                error_codes::STORAGE_CORRUPTED,
                                format!("decode EverQuest action-prior eval row {key}: {error}"),
                            )
                        })?;
                    if row.profile_id != profile_id || row.sample_id != *sample_id {
                        return Err(mcp_error(
                            error_codes::STORAGE_CORRUPTED,
                            format!("EverQuest action-prior eval row key/body mismatch: {key}"),
                        ));
                    }
                    Ok(row)
                })
                .collect::<Result<Vec<_>, ErrorData>>()?
        };
        Ok(rows)
    }

    fn persist_kv_json<T>(&self, key: &str, row: &T, label: &str) -> Result<(T, u64), ErrorData>
    where
        T: DeserializeOwned + Serialize,
    {
        let encoded = serde_json::to_vec(row).map_err(|error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!("encode {label}: {error}"),
            )
        })?;
        let stored = {
            let runtime = self.reflex_runtime()?;
            let runtime = runtime.lock().map_err(|_error| {
                mcp_error(
                    error_codes::TOOL_INTERNAL_ERROR,
                    format!("reflex runtime lock poisoned while writing {label}"),
                )
            })?;
            runtime
                .storage_put_kv_rows(vec![(key.as_bytes().to_vec(), encoded)])
                .map_err(|error| {
                    mcp_error(
                        error_codes::STORAGE_WRITE_FAILED,
                        format!("write {label}: {error}"),
                    )
                })?;
            runtime
                .storage_kv_row(key.as_bytes())
                .map_err(|error| {
                    mcp_error(
                        error_codes::STORAGE_READ_FAILED,
                        format!("read {label} after write: {error}"),
                    )
                })?
                .ok_or_else(|| {
                    mcp_error(
                        error_codes::STORAGE_READ_FAILED,
                        format!("{label} missing after write"),
                    )
                })?
        };
        let readback = serde_json::from_slice::<T>(&stored).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_CORRUPTED,
                format!("decode {label} after write: {error}"),
            )
        })?;
        Ok((readback, len_to_u64(stored.len())))
    }
}

fn eval_row_from_params(
    params: EverQuestActionPriorRecordParams,
) -> Result<EverQuestActionPriorEvalRow, ErrorData> {
    let sample_id = validate_id("sample_id", &params.sample_id)?;
    let profile_id = validate_everquest_profile_id(&params.profile_id)?;
    let prediction_id = validate_id("prediction_id", &params.prediction_id)?;
    let actual_outcome_id = params
        .actual_outcome_id
        .map(|value| validate_id("actual_outcome_id", &value))
        .transpose()?;
    let prediction = normalize_prediction(params.prediction)?;
    let actual = normalize_actual(params.actual)?;
    let source_episode_ids = normalize_id_vec(
        "source_episode_ids",
        params.source_episode_ids,
        MAX_SOURCE_EPISODE_IDS,
    )?;
    let source_refs = normalize_source_refs(params.source_refs)?;
    let limitations = normalize_text_vec("limitations", params.limitations, MAX_LIMITATIONS)?;
    let correctness = correctness_for(&prediction, &actual);
    Ok(EverQuestActionPriorEvalRow {
        schema_version: SCHEMA_VERSION,
        row_kind: "everquest_action_prior_eval".to_owned(),
        profile_id,
        sample_id,
        prediction_id,
        actual_outcome_id,
        recorded_at: Utc::now(),
        prediction,
        actual,
        correctness,
        source_episode_ids,
        source_refs,
        limitations,
        evidence_boundary: evidence_boundary(),
    })
}

fn normalize_scorecard_params(
    params: EverQuestActionPriorScorecardParams,
) -> Result<EverQuestActionPriorScorecardParams, ErrorData> {
    let window_id = validate_id("window_id", &params.window_id)?;
    let profile_id = validate_everquest_profile_id(&params.profile_id)?;
    if params.sample_ids.len() > MAX_SCORECARD_SAMPLES {
        return Err(params_error(format!(
            "sample_ids must contain <= {MAX_SCORECARD_SAMPLES} ids"
        )));
    }
    if params.min_samples == 0 {
        return Err(params_error("min_samples must be >= 1"));
    }
    validate_unit_interval(
        "min_confidence_for_action",
        params.min_confidence_for_action,
    )?;
    validate_unit_interval("competence_floor", params.competence_floor)?;
    validate_unit_interval("stretch_target", params.stretch_target)?;
    if params.stretch_target < params.competence_floor {
        return Err(params_error(
            "stretch_target must be >= competence_floor for the floor-not-ceiling contract",
        ));
    }
    let sample_ids = normalize_id_vec("sample_ids", params.sample_ids, MAX_SCORECARD_SAMPLES)?;
    let mut seen = BTreeSet::new();
    for sample_id in &sample_ids {
        if !seen.insert(sample_id.clone()) {
            return Err(params_error(format!(
                "sample_ids must not contain duplicate id {sample_id:?}"
            )));
        }
    }
    let limitations = normalize_text_vec("limitations", params.limitations, MAX_LIMITATIONS)?;
    Ok(EverQuestActionPriorScorecardParams {
        window_id,
        profile_id,
        sample_ids,
        min_samples: params.min_samples,
        min_confidence_for_action: params.min_confidence_for_action,
        competence_floor: params.competence_floor,
        stretch_target: params.stretch_target,
        limitations,
    })
}

fn normalize_prediction(
    prediction: EverQuestActionPriorPrediction,
) -> Result<EverQuestActionPriorPrediction, ErrorData> {
    validate_unit_interval("prediction.confidence", prediction.confidence)?;
    let normalized = EverQuestActionPriorPrediction {
        next_action: normalize_optional_text("prediction.next_action", prediction.next_action)?,
        top3_actions: normalize_text_vec(
            "prediction.top3_actions",
            prediction.top3_actions,
            MAX_TOP3_ACTIONS,
        )?,
        zone_short_name: normalize_optional_text(
            "prediction.zone_short_name",
            prediction.zone_short_name,
        )?,
        coord_bucket: normalize_optional_text("prediction.coord_bucket", prediction.coord_bucket)?,
        hazard_avoidance: prediction.hazard_avoidance,
        confidence: prediction.confidence,
        abstain: prediction.abstain,
    };
    if !normalized.abstain && !has_prediction_signal(&normalized) {
        return Err(params_error(
            "non-abstaining prediction must include at least one predicted field",
        ));
    }
    Ok(normalized)
}

fn normalize_actual(
    actual: EverQuestActionPriorActual,
) -> Result<EverQuestActionPriorActual, ErrorData> {
    Ok(EverQuestActionPriorActual {
        next_action: normalize_optional_text("actual.next_action", actual.next_action)?,
        zone_short_name: normalize_optional_text("actual.zone_short_name", actual.zone_short_name)?,
        coord_bucket: normalize_optional_text("actual.coord_bucket", actual.coord_bucket)?,
        hazard_occurred: actual.hazard_occurred,
        surprise: actual.surprise,
    })
}

fn normalize_source_refs(
    source_refs: Vec<EverQuestActionPriorSourceRef>,
) -> Result<Vec<EverQuestActionPriorSourceRef>, ErrorData> {
    if source_refs.len() > MAX_SOURCE_REFS {
        return Err(params_error(format!(
            "source_refs must contain <= {MAX_SOURCE_REFS} refs"
        )));
    }
    source_refs
        .into_iter()
        .map(|source_ref| {
            Ok(EverQuestActionPriorSourceRef {
                kind: normalize_required_text("source_refs.kind", &source_ref.kind)?,
                row_key: normalize_optional_text("source_refs.row_key", source_ref.row_key)?,
                path: normalize_optional_text("source_refs.path", source_ref.path)?,
                start_offset: source_ref.start_offset,
                next_offset: source_ref.next_offset,
                note: normalize_optional_text("source_refs.note", source_ref.note)?,
            })
        })
        .collect()
}

fn scorecard_row_from_samples(
    params: &EverQuestActionPriorScorecardParams,
    samples: &[EverQuestActionPriorEvalRow],
) -> EverQuestActionPriorScorecardRow {
    let metrics = metrics_for_samples(params, samples);
    let calibration_buckets = calibration_buckets_for(samples);
    let competence = competence_for(params, &metrics);
    EverQuestActionPriorScorecardRow {
        schema_version: SCHEMA_VERSION,
        row_kind: "everquest_action_prior_scorecard".to_owned(),
        profile_id: params.profile_id.clone(),
        window_id: params.window_id.clone(),
        generated_at: Utc::now(),
        window_bounds: window_bounds_for_samples(samples),
        source_episode_ids: source_episode_ids_for_samples(samples),
        source_sample_keys: params
            .sample_ids
            .iter()
            .map(|sample_id| eval_row_key(&params.profile_id, sample_id))
            .collect(),
        metrics,
        calibration_buckets,
        competence,
        limitations: params.limitations.clone(),
        evidence_boundary: evidence_boundary(),
    }
}

#[allow(clippy::too_many_lines)]
fn metrics_for_samples(
    params: &EverQuestActionPriorScorecardParams,
    samples: &[EverQuestActionPriorEvalRow],
) -> EverQuestActionPriorMetrics {
    let mut metrics = EverQuestActionPriorMetrics {
        sample_count: len_to_u32(samples.len()),
        min_samples: params.min_samples,
        evaluated_count: 0,
        abstention_count: 0,
        honest_abstention_count: 0,
        unknown_actual_count: 0,
        low_confidence_action_count: 0,
        overconfident_wrong_count: 0,
        surprise_count: 0,
        surprise_rate: None,
        top1_total: 0,
        top1_correct: 0,
        top1_accuracy: None,
        top3_total: 0,
        top3_correct: 0,
        top3_accuracy: None,
        zone_total: 0,
        zone_correct: 0,
        zone_accuracy: None,
        coord_bucket_total: 0,
        coord_bucket_correct: 0,
        coord_bucket_accuracy: None,
        hazard_avoidance_total: 0,
        hazard_avoidance_correct: 0,
        hazard_avoidance_accuracy: None,
        useful_total: 0,
        useful_correct: 0,
        useful_accuracy: None,
        supervised_utility_rate: None,
    };

    for sample in samples {
        if sample.correctness.abstained {
            metrics.abstention_count = metrics.abstention_count.saturating_add(1);
            if sample.prediction.confidence < params.min_confidence_for_action
                || !sample.correctness.actual_known
            {
                metrics.honest_abstention_count = metrics.honest_abstention_count.saturating_add(1);
            }
        }
        if !sample.correctness.actual_known {
            metrics.unknown_actual_count = metrics.unknown_actual_count.saturating_add(1);
        }
        if sample.actual.surprise {
            metrics.surprise_count = metrics.surprise_count.saturating_add(1);
        }
        if sample.correctness.overconfident_wrong {
            metrics.overconfident_wrong_count = metrics.overconfident_wrong_count.saturating_add(1);
        }
        if !sample.correctness.abstained
            && sample.correctness.actual_known
            && sample.prediction.confidence < params.min_confidence_for_action
        {
            metrics.low_confidence_action_count =
                metrics.low_confidence_action_count.saturating_add(1);
        }
        if !sample.correctness.abstained && sample.correctness.actual_known {
            metrics.evaluated_count = metrics.evaluated_count.saturating_add(1);
            metrics.useful_total = metrics.useful_total.saturating_add(1);
            if sample.correctness.useful {
                metrics.useful_correct = metrics.useful_correct.saturating_add(1);
            }
        }
        if !sample.correctness.abstained && sample.actual.next_action.is_some() {
            metrics.top1_total = metrics.top1_total.saturating_add(1);
            metrics.top3_total = metrics.top3_total.saturating_add(1);
            if sample.correctness.top1_correct == Some(true) {
                metrics.top1_correct = metrics.top1_correct.saturating_add(1);
            }
            if sample.correctness.top3_correct == Some(true) {
                metrics.top3_correct = metrics.top3_correct.saturating_add(1);
            }
        }
        if !sample.correctness.abstained && sample.actual.zone_short_name.is_some() {
            metrics.zone_total = metrics.zone_total.saturating_add(1);
            if sample.correctness.zone_correct == Some(true) {
                metrics.zone_correct = metrics.zone_correct.saturating_add(1);
            }
        }
        if !sample.correctness.abstained && sample.actual.coord_bucket.is_some() {
            metrics.coord_bucket_total = metrics.coord_bucket_total.saturating_add(1);
            if sample.correctness.coord_bucket_correct == Some(true) {
                metrics.coord_bucket_correct = metrics.coord_bucket_correct.saturating_add(1);
            }
        }
        if !sample.correctness.abstained && sample.actual.hazard_occurred.is_some() {
            metrics.hazard_avoidance_total = metrics.hazard_avoidance_total.saturating_add(1);
            if sample.correctness.hazard_avoidance_correct == Some(true) {
                metrics.hazard_avoidance_correct =
                    metrics.hazard_avoidance_correct.saturating_add(1);
            }
        }
    }

    metrics.surprise_rate = ratio(metrics.surprise_count, metrics.sample_count);
    metrics.top1_accuracy = ratio(metrics.top1_correct, metrics.top1_total);
    metrics.top3_accuracy = ratio(metrics.top3_correct, metrics.top3_total);
    metrics.zone_accuracy = ratio(metrics.zone_correct, metrics.zone_total);
    metrics.coord_bucket_accuracy = ratio(metrics.coord_bucket_correct, metrics.coord_bucket_total);
    metrics.hazard_avoidance_accuracy = ratio(
        metrics.hazard_avoidance_correct,
        metrics.hazard_avoidance_total,
    );
    metrics.useful_accuracy = ratio(metrics.useful_correct, metrics.useful_total);
    metrics.supervised_utility_rate = ratio(
        metrics
            .useful_correct
            .saturating_add(metrics.honest_abstention_count),
        metrics.sample_count,
    );
    metrics
}

fn calibration_buckets_for(
    samples: &[EverQuestActionPriorEvalRow],
) -> Vec<EverQuestActionPriorCalibrationBucket> {
    let mut buckets = [
        CalibrationAccumulator::new("0.00-0.20", 0.0, Some(0.2)),
        CalibrationAccumulator::new("0.20-0.40", 0.2, Some(0.4)),
        CalibrationAccumulator::new("0.40-0.60", 0.4, Some(0.6)),
        CalibrationAccumulator::new("0.60-0.80", 0.6, Some(0.8)),
        CalibrationAccumulator::new("0.80-1.00", 0.8, None),
    ];
    for sample in samples {
        let bucket = confidence_bucket_index(sample.prediction.confidence);
        buckets[bucket].record(sample);
    }
    buckets
        .into_iter()
        .map(CalibrationAccumulator::finish)
        .collect()
}

fn competence_for(
    params: &EverQuestActionPriorScorecardParams,
    metrics: &EverQuestActionPriorMetrics,
) -> EverQuestActionPriorCompetence {
    let useful_accuracy = metrics.useful_accuracy.unwrap_or(0.0);
    let has_low_confidence_action = metrics.low_confidence_action_count > 0;
    let meets_stretch_target = metrics.sample_count >= params.min_samples
        && !has_low_confidence_action
        && useful_accuracy >= params.stretch_target;
    let meets_minimum_floor = metrics.sample_count >= params.min_samples
        && !has_low_confidence_action
        && useful_accuracy >= params.competence_floor;
    let status = if metrics.sample_count == 0 {
        "no_verified_trajectories"
    } else if metrics.sample_count < params.min_samples {
        "insufficient_samples"
    } else if metrics.evaluated_count == 0 {
        "no_evaluated_predictions"
    } else if has_low_confidence_action {
        "low_confidence_action_forced"
    } else if meets_stretch_target {
        "stretch_target_met"
    } else if meets_minimum_floor {
        "minimum_competence_floor_met"
    } else {
        "below_minimum_competence_floor"
    };
    let recommendation = if metrics.sample_count == 0 {
        "collect_verified_trajectories_before_claiming_competence"
    } else if metrics.sample_count < params.min_samples {
        "collect_more_verified_samples_before_using_score"
    } else if has_low_confidence_action {
        "require_abstention_for_low_confidence_actions"
    } else if metrics.overconfident_wrong_count > 0 {
        "fix_or_downweight_overconfident_wrong_predictions"
    } else if metrics.evaluated_count == 0 {
        "improve_state_coverage_or_keep_abstaining"
    } else if meets_stretch_target {
        "use_supervised_and_continue_optimizing_above_the_floor"
    } else if meets_minimum_floor {
        "minimum_useful_floor_met_continue_optimizing"
    } else {
        "abstain_on_low_confidence_and_collect_better_trajectories"
    };
    EverQuestActionPriorCompetence {
        minimum_floor: params.competence_floor,
        stretch_target: params.stretch_target,
        status: status.to_owned(),
        meets_minimum_floor,
        meets_stretch_target,
        minimum_is_floor_not_ceiling: true,
        optimization_target:
            "maximize_verified_supervised_performance_without_forced_low_confidence_actions"
                .to_owned(),
        recommendation: recommendation.to_owned(),
    }
}

fn correctness_for(
    prediction: &EverQuestActionPriorPrediction,
    actual: &EverQuestActionPriorActual,
) -> EverQuestActionPriorCorrectness {
    let actual_known = actual.next_action.is_some()
        || actual.zone_short_name.is_some()
        || actual.coord_bucket.is_some()
        || actual.hazard_occurred.is_some();
    let top1_correct = actual.next_action.as_deref().map(|actual_action| {
        prediction
            .next_action
            .as_deref()
            .is_some_and(|predicted| same_label(predicted, actual_action))
    });
    let top3_correct = actual.next_action.as_deref().map(|actual_action| {
        prediction
            .top3_actions
            .iter()
            .any(|predicted| same_label(predicted, actual_action))
    });
    let zone_correct = actual.zone_short_name.as_deref().map(|actual_zone| {
        prediction
            .zone_short_name
            .as_deref()
            .is_some_and(|predicted| same_label(predicted, actual_zone))
    });
    let coord_bucket_correct = actual.coord_bucket.as_deref().map(|actual_bucket| {
        prediction
            .coord_bucket
            .as_deref()
            .is_some_and(|predicted| same_label(predicted, actual_bucket))
    });
    let hazard_avoidance_correct = actual
        .hazard_occurred
        .map(|hazard_occurred| prediction.hazard_avoidance == Some(!hazard_occurred));
    let useful = !prediction.abstain
        && actual_known
        && [
            top1_correct,
            top3_correct,
            zone_correct,
            coord_bucket_correct,
            hazard_avoidance_correct,
        ]
        .into_iter()
        .any(|value| value == Some(true));
    let overconfident_wrong = !prediction.abstain
        && actual_known
        && !useful
        && prediction.confidence >= OVERCONFIDENT_WRONG_THRESHOLD;
    let class = if prediction.abstain {
        "abstained"
    } else if !actual_known {
        "unknown_actual"
    } else if top1_correct == Some(true) {
        "correct_top1"
    } else if top3_correct == Some(true) {
        "correct_top3"
    } else if useful {
        "correct_context"
    } else {
        "wrong"
    };
    EverQuestActionPriorCorrectness {
        class: class.to_owned(),
        abstained: prediction.abstain,
        actual_known,
        top1_correct,
        top3_correct,
        zone_correct,
        coord_bucket_correct,
        hazard_avoidance_correct,
        useful,
        confidence_bucket: confidence_bucket_label(prediction.confidence).to_owned(),
        overconfident_wrong,
    }
}

#[derive(Clone, Debug)]
struct CalibrationAccumulator {
    bucket: &'static str,
    lower_inclusive: f32,
    upper_exclusive: Option<f32>,
    sample_count: u32,
    evaluated_count: u32,
    useful_correct: u32,
    abstention_count: u32,
    confidence_sum: f32,
}

impl CalibrationAccumulator {
    const fn new(bucket: &'static str, lower_inclusive: f32, upper_exclusive: Option<f32>) -> Self {
        Self {
            bucket,
            lower_inclusive,
            upper_exclusive,
            sample_count: 0,
            evaluated_count: 0,
            useful_correct: 0,
            abstention_count: 0,
            confidence_sum: 0.0,
        }
    }

    fn record(&mut self, sample: &EverQuestActionPriorEvalRow) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.confidence_sum += sample.prediction.confidence;
        if sample.correctness.abstained {
            self.abstention_count = self.abstention_count.saturating_add(1);
            return;
        }
        if sample.correctness.actual_known {
            self.evaluated_count = self.evaluated_count.saturating_add(1);
            if sample.correctness.useful {
                self.useful_correct = self.useful_correct.saturating_add(1);
            }
        }
    }

    fn finish(self) -> EverQuestActionPriorCalibrationBucket {
        EverQuestActionPriorCalibrationBucket {
            bucket: self.bucket.to_owned(),
            lower_inclusive: self.lower_inclusive,
            upper_exclusive: self.upper_exclusive,
            sample_count: self.sample_count,
            evaluated_count: self.evaluated_count,
            useful_correct: self.useful_correct,
            abstention_count: self.abstention_count,
            mean_confidence: if self.sample_count == 0 {
                None
            } else {
                Some(mean_confidence(self.confidence_sum, self.sample_count))
            },
            useful_accuracy: ratio(self.useful_correct, self.evaluated_count),
        }
    }
}

fn window_bounds_for_samples(
    samples: &[EverQuestActionPriorEvalRow],
) -> EverQuestActionPriorWindowBounds {
    EverQuestActionPriorWindowBounds {
        first_sample_recorded_at: samples.iter().map(|sample| sample.recorded_at).min(),
        last_sample_recorded_at: samples.iter().map(|sample| sample.recorded_at).max(),
    }
}

fn source_episode_ids_for_samples(samples: &[EverQuestActionPriorEvalRow]) -> Vec<String> {
    let mut source_episode_ids = BTreeSet::new();
    for sample in samples {
        for source_episode_id in &sample.source_episode_ids {
            source_episode_ids.insert(source_episode_id.clone());
        }
    }
    source_episode_ids.into_iter().collect()
}

fn validate_everquest_profile_id(value: &str) -> Result<String, ErrorData> {
    let profile_id = normalize_required_text("profile_id", value)?;
    if profile_id != EVERQUEST_PROFILE_ID {
        return Err(params_error(format!(
            "profile_id must be {EVERQUEST_PROFILE_ID:?}; got {profile_id:?}"
        )));
    }
    Ok(profile_id)
}

fn validate_id(field: &str, value: &str) -> Result<String, ErrorData> {
    let value = value.trim();
    if value.is_empty() {
        return Err(params_error(format!("{field} must not be empty")));
    }
    if value.len() > MAX_ID_BYTES {
        return Err(params_error(format!(
            "{field} must be <= {MAX_ID_BYTES} bytes"
        )));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(params_error(format!(
            "{field} may contain only ASCII letters, digits, '.', '_', and '-'"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_id_vec(
    field: &str,
    values: Vec<String>,
    max_values: usize,
) -> Result<Vec<String>, ErrorData> {
    if values.len() > max_values {
        return Err(params_error(format!(
            "{field} must contain <= {max_values} values"
        )));
    }
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| validate_id(&format!("{field}[{index}]"), &value))
        .collect()
}

fn normalize_text_vec(
    field: &str,
    values: Vec<String>,
    max_values: usize,
) -> Result<Vec<String>, ErrorData> {
    if values.len() > max_values {
        return Err(params_error(format!(
            "{field} must contain <= {max_values} values"
        )));
    }
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| normalize_required_text(&format!("{field}[{index}]"), &value))
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

fn normalize_required_text(field: &str, value: &str) -> Result<String, ErrorData> {
    let value = value.trim();
    if value.is_empty() {
        return Err(params_error(format!(
            "{field} must not be empty when present"
        )));
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

fn validate_unit_interval(field: &str, value: f32) -> Result<(), ErrorData> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(params_error(format!(
            "{field} must be a finite value between 0.0 and 1.0"
        )));
    }
    Ok(())
}

const fn has_prediction_signal(prediction: &EverQuestActionPriorPrediction) -> bool {
    prediction.next_action.is_some()
        || !prediction.top3_actions.is_empty()
        || prediction.zone_short_name.is_some()
        || prediction.coord_bucket.is_some()
        || prediction.hazard_avoidance.is_some()
}

fn evidence_boundary() -> EverQuestActionPriorEvidenceBoundary {
    EverQuestActionPriorEvidenceBoundary {
        supports_planning_quality: true,
        manual_fsv_required_for_runtime: true,
        is_fsv: false,
        redacted: true,
        note: "Scorecards support supervised planning quality only; manual FSV at physical SoT still gates runtime/action claims."
            .to_owned(),
    }
}

fn eval_row_key(profile_id: &str, sample_id: &str) -> String {
    format!("{EVAL_ROW_PREFIX}/{profile_id}/{sample_id}")
}

fn scorecard_row_key(profile_id: &str, window_id: &str) -> String {
    format!("{SCORECARD_ROW_PREFIX}/{profile_id}/{window_id}")
}

fn same_label(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn confidence_bucket_index(confidence: f32) -> usize {
    if confidence < 0.2 {
        0
    } else if confidence < 0.4 {
        1
    } else if confidence < 0.6 {
        2
    } else if confidence < 0.8 {
        3
    } else {
        4
    }
}

fn confidence_bucket_label(confidence: f32) -> &'static str {
    match confidence_bucket_index(confidence) {
        0 => "0.00-0.20",
        1 => "0.20-0.40",
        2 => "0.40-0.60",
        3 => "0.60-0.80",
        _ => "0.80-1.00",
    }
}

fn ratio(numerator: u32, denominator: u32) -> Option<f32> {
    (denominator > 0).then(|| ratio_f32(numerator, denominator))
}

#[allow(clippy::cast_precision_loss)]
fn ratio_f32(numerator: u32, denominator: u32) -> f32 {
    numerator as f32 / denominator as f32
}

#[allow(clippy::cast_precision_loss)]
fn mean_confidence(confidence_sum: f32, sample_count: u32) -> f32 {
    confidence_sum / sample_count as f32
}

fn len_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn len_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn default_profile_id() -> String {
    EVERQUEST_PROFILE_ID.to_owned()
}

const fn default_min_samples() -> u32 {
    DEFAULT_MIN_SAMPLES
}

const fn default_min_confidence_for_action() -> f32 {
    DEFAULT_MIN_CONFIDENCE_FOR_ACTION
}

const fn default_competence_floor() -> f32 {
    DEFAULT_COMPETENCE_FLOOR
}

const fn default_stretch_target() -> f32 {
    DEFAULT_STRETCH_TARGET
}

fn params_error(message: impl Into<String>) -> ErrorData {
    mcp_error(error_codes::TOOL_PARAMS_INVALID, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prediction(next_action: &str, confidence: f32) -> EverQuestActionPriorPrediction {
        EverQuestActionPriorPrediction {
            next_action: Some(next_action.to_owned()),
            top3_actions: vec![next_action.to_owned()],
            zone_short_name: Some("neriaka".to_owned()),
            coord_bucket: Some("neriaka-bank".to_owned()),
            hazard_avoidance: Some(true),
            confidence,
            abstain: false,
        }
    }

    fn actual(next_action: &str) -> EverQuestActionPriorActual {
        EverQuestActionPriorActual {
            next_action: Some(next_action.to_owned()),
            zone_short_name: Some("neriaka".to_owned()),
            coord_bucket: Some("neriaka-bank".to_owned()),
            hazard_occurred: Some(false),
            surprise: false,
        }
    }

    fn actual_far(next_action: &str) -> EverQuestActionPriorActual {
        EverQuestActionPriorActual {
            next_action: Some(next_action.to_owned()),
            zone_short_name: Some("nektulos".to_owned()),
            coord_bucket: Some("nektulos-entry".to_owned()),
            hazard_occurred: Some(true),
            surprise: true,
        }
    }

    fn row(
        sample_id: &str,
        prediction: EverQuestActionPriorPrediction,
        actual: EverQuestActionPriorActual,
    ) -> EverQuestActionPriorEvalRow {
        let correctness = correctness_for(&prediction, &actual);
        EverQuestActionPriorEvalRow {
            schema_version: SCHEMA_VERSION,
            row_kind: "everquest_action_prior_eval".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            sample_id: sample_id.to_owned(),
            prediction_id: format!("pred-{sample_id}"),
            actual_outcome_id: Some(format!("actual-{sample_id}")),
            recorded_at: Utc::now(),
            prediction,
            actual,
            correctness,
            source_episode_ids: Vec::new(),
            source_refs: Vec::new(),
            limitations: Vec::new(),
            evidence_boundary: evidence_boundary(),
        }
    }

    #[test]
    fn correctness_distinguishes_top1_top3_and_wrong() {
        let top1 = correctness_for(&prediction("inventory", 0.7), &actual("inventory"));
        assert_eq!(top1.class, "correct_top1");
        assert!(top1.useful);

        let mut top3_prediction = prediction("forward", 0.7);
        top3_prediction.top3_actions = vec!["forward".to_owned(), "inventory".to_owned()];
        let top3 = correctness_for(&top3_prediction, &actual("inventory"));
        assert_eq!(top3.class, "correct_top3");
        assert_eq!(top3.top1_correct, Some(false));
        assert_eq!(top3.top3_correct, Some(true));

        let wrong = correctness_for(&prediction("forward", 0.95), &actual_far("inventory"));
        assert_eq!(wrong.class, "wrong");
        assert!(wrong.overconfident_wrong);
    }

    #[test]
    fn scorecard_floor_is_minimum_not_ceiling() {
        let samples = vec![
            row("s1", prediction("inventory", 0.9), actual("inventory")),
            row("s2", prediction("target_self", 0.8), actual("target_self")),
            row("s3", prediction("sit", 0.7), actual("sit")),
        ];
        let params = EverQuestActionPriorScorecardParams {
            window_id: "unit-window".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            sample_ids: samples
                .iter()
                .map(|sample| sample.sample_id.clone())
                .collect(),
            min_samples: 3,
            min_confidence_for_action: DEFAULT_MIN_CONFIDENCE_FOR_ACTION,
            competence_floor: DEFAULT_COMPETENCE_FLOOR,
            stretch_target: DEFAULT_STRETCH_TARGET,
            limitations: Vec::new(),
        };
        let scorecard = scorecard_row_from_samples(&params, &samples);
        assert_eq!(scorecard.metrics.useful_accuracy, Some(1.0));
        assert_eq!(scorecard.competence.status, "stretch_target_met");
        assert!(scorecard.competence.minimum_is_floor_not_ceiling);
    }

    #[test]
    fn no_samples_scorecard_abstains_without_claiming_competence() {
        let params = EverQuestActionPriorScorecardParams {
            window_id: "empty-window".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            sample_ids: Vec::new(),
            min_samples: 3,
            min_confidence_for_action: DEFAULT_MIN_CONFIDENCE_FOR_ACTION,
            competence_floor: DEFAULT_COMPETENCE_FLOOR,
            stretch_target: DEFAULT_STRETCH_TARGET,
            limitations: Vec::new(),
        };
        let scorecard = scorecard_row_from_samples(&params, &[]);
        assert_eq!(scorecard.metrics.sample_count, 0);
        assert_eq!(scorecard.competence.status, "no_verified_trajectories");
        assert!(!scorecard.competence.meets_minimum_floor);
    }

    #[test]
    fn low_confidence_actions_do_not_meet_competence_floor() {
        let samples = vec![
            row("s1", prediction("inventory", 0.1), actual("inventory")),
            row("s2", prediction("target_self", 0.2), actual("target_self")),
            row("s3", prediction("sit", 0.3), actual("sit")),
        ];
        let params = EverQuestActionPriorScorecardParams {
            window_id: "low-confidence-window".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            sample_ids: samples
                .iter()
                .map(|sample| sample.sample_id.clone())
                .collect(),
            min_samples: 3,
            min_confidence_for_action: DEFAULT_MIN_CONFIDENCE_FOR_ACTION,
            competence_floor: DEFAULT_COMPETENCE_FLOOR,
            stretch_target: DEFAULT_STRETCH_TARGET,
            limitations: Vec::new(),
        };
        let scorecard = scorecard_row_from_samples(&params, &samples);
        assert_eq!(scorecard.metrics.useful_accuracy, Some(1.0));
        assert_eq!(scorecard.metrics.low_confidence_action_count, 3);
        assert_eq!(scorecard.competence.status, "low_confidence_action_forced");
        assert!(!scorecard.competence.meets_minimum_floor);
    }
}
