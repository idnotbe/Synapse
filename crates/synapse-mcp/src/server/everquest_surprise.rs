mod compare;
mod model;
mod validation;

use rmcp::ErrorData;
use serde_json::Value;

use self::{
    compare::{compare_prediction, observed_from_current_state, remediation_for},
    model::{
        EverQuestSurpriseDetectParams, EverQuestSurpriseDetectResponse, EverQuestSurprisePayload,
        TOOL, WORLD_MODEL_MAX_PAYLOAD_BYTES,
    },
    validation::{decode_json_row, normalize_params, source_refs_for_payload},
};
use super::{
    Json, Parameters, SynapseService,
    everquest_state::EverQuestCurrentState,
    everquest_world_model::model::{
        EverQuestWorldModelKind, EverQuestWorldModelRecordParams,
        EverQuestWorldModelRetentionClass, EverQuestWorldModelSourceRef,
        EverQuestWorldModelWriteMode,
    },
    tool, tool_router,
};
use synapse_core::error_codes;

use crate::m1::mcp_error;

#[tool_router(router = everquest_surprise_tool_router, vis = "pub(super)")]
impl SynapseService {
    #[tool(
        description = "Compare predicted EverQuest outcome with observed state/log evidence and persist a compact surprise world-model row"
    )]
    pub async fn everquest_surprise_detect(
        &self,
        params: Parameters<EverQuestSurpriseDetectParams>,
    ) -> Result<Json<EverQuestSurpriseDetectResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = TOOL,
            "tool.invocation kind=everquest_surprise_detect"
        );
        let params = normalize_params(params.0)?;
        let payload = self.surprise_payload(&params)?;
        let decision = payload.comparison.decision.clone();
        let surprise_detected = payload.comparison.surprise_detected;
        let stop_condition = payload.comparison.stop_condition;
        let world_model = self.persist_surprise_payload(&params, payload)?;
        Ok(Json(EverQuestSurpriseDetectResponse {
            ok: true,
            row_key: world_model.row_key.clone(),
            stored_value_len_bytes: world_model.stored_value_len_bytes,
            decision,
            surprise_detected,
            stop_condition,
            world_model,
        }))
    }
}

impl SynapseService {
    fn surprise_payload(
        &self,
        params: &EverQuestSurpriseDetectParams,
    ) -> Result<EverQuestSurprisePayload, ErrorData> {
        let observed = self.read_observed_surprise_state(params)?;
        let comparison = compare_prediction(params.prediction.as_ref(), &observed, params);
        Ok(EverQuestSurprisePayload {
            schema_version: model::SCHEMA_VERSION,
            row_kind: "everquest_surprise_event".to_owned(),
            surprise_id: params.surprise_id.clone(),
            detected_at: chrono::Utc::now(),
            prediction: params.prediction.clone(),
            remediation: remediation_for(&comparison),
            observed,
            comparison,
            evidence_boundary: model::EverQuestSurpriseEvidenceBoundary {
                writes_world_model_row_only: true,
                executes_input: false,
                compact_redacted: true,
                manual_fsv_required_for_runtime: true,
                is_fsv_script: false,
                note: "Surprise rows stop or repair planner state only; gameplay claims still require manual physical UI/log/storage FSV."
                    .to_owned(),
            },
        })
    }

    fn read_observed_surprise_state(
        &self,
        params: &EverQuestSurpriseDetectParams,
    ) -> Result<model::EverQuestSurpriseObserved, ErrorData> {
        if let Some(override_state) = &params.observed_override {
            return Ok(model::EverQuestSurpriseObserved {
                source_mode: override_state
                    .source_mode
                    .clone()
                    .unwrap_or_else(|| "observed_override".to_owned()),
                observed_state_row_key: params.observed_state_row_key.clone(),
                observed_outcome_id: override_state.observed_outcome_id.clone(),
                observed_zone_short_name: override_state.observed_zone_short_name.clone(),
                observed_outcome_kind: override_state.observed_outcome_kind.clone(),
                observed_at: override_state.observed_at,
                zone_confidence: override_state.zone_confidence,
                outcome_confidence: override_state.outcome_confidence,
                source_refs: override_state.source_refs.clone(),
            });
        }

        let stored = {
            let runtime = self.reflex_runtime()?;
            let runtime = runtime.lock().map_err(|_| {
                mcp_error(
                    error_codes::TOOL_INTERNAL_ERROR,
                    "reflex runtime lock poisoned while reading EverQuest surprise observed state",
                )
            })?;
            runtime
                .storage_kv_row(params.observed_state_row_key.as_bytes())
                .map_err(|error| mcp_error(error.code(), error.to_string()))?
        };
        let Some(stored) = stored else {
            return Ok(model::EverQuestSurpriseObserved {
                source_mode: "current_state_row_missing".to_owned(),
                observed_state_row_key: params.observed_state_row_key.clone(),
                observed_outcome_id: None,
                observed_zone_short_name: None,
                observed_outcome_kind: None,
                observed_at: None,
                zone_confidence: 0.0,
                outcome_confidence: 0.0,
                source_refs: vec![EverQuestWorldModelSourceRef {
                    kind: "current_state_row_missing".to_owned(),
                    row_key: Some(params.observed_state_row_key.clone()),
                    path: None,
                    start_offset: None,
                    next_offset: None,
                    content_sha256: None,
                    summary: Some("state row absent during surprise comparison".to_owned()),
                }],
            });
        };
        let state =
            decode_json_row::<EverQuestCurrentState>(&stored, "EverQuest current-state row")?;
        Ok(observed_from_current_state(
            &params.observed_state_row_key,
            &state,
        ))
    }

    fn persist_surprise_payload(
        &self,
        params: &EverQuestSurpriseDetectParams,
        payload: EverQuestSurprisePayload,
    ) -> Result<super::everquest_world_model::model::EverQuestWorldModelRecordResponse, ErrorData>
    {
        let refs = source_refs_for_payload(params, &payload);
        let payload_value = serde_json::to_value(payload).map_err(|error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!("encode EverQuest surprise payload: {error}"),
            )
        })?;
        reject_empty_payload(&payload_value)?;
        let record = EverQuestWorldModelRecordParams {
            row_kind: EverQuestWorldModelKind::Surprise,
            row_id: params.surprise_id.clone(),
            profile_id: params.profile_id.clone(),
            payload: payload_value,
            source_refs: refs,
            write_mode: EverQuestWorldModelWriteMode::Create,
            retention_class: EverQuestWorldModelRetentionClass::Episode,
            compact_redacted: true,
            max_payload_bytes: WORLD_MODEL_MAX_PAYLOAD_BYTES,
        };
        self.record_world_model_params(record)
    }
}

fn reject_empty_payload(payload: &Value) -> Result<(), ErrorData> {
    if payload.as_object().is_none_or(serde_json::Map::is_empty) {
        return Err(mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            "EverQuest surprise payload unexpectedly empty",
        ));
    }
    Ok(())
}
