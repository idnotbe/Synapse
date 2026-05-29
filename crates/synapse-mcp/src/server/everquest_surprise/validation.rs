use rmcp::ErrorData;
use serde::de::DeserializeOwned;
use synapse_core::error_codes;

use super::model::{
    EverQuestSurpriseDetectParams, EverQuestSurpriseObservedOverride, EverQuestSurprisePayload,
    EverQuestSurprisePrediction, MAX_SOURCE_REFS, MAX_TEXT_BYTES,
};
use crate::{
    m1::mcp_error,
    server::{
        everquest_log::EVERQUEST_PROFILE_ID,
        everquest_world_model::model::EverQuestWorldModelSourceRef,
    },
};

pub(super) fn normalize_params(
    mut params: EverQuestSurpriseDetectParams,
) -> Result<EverQuestSurpriseDetectParams, ErrorData> {
    params.profile_id = validate_profile_id(&params.profile_id)?;
    params.surprise_id = validate_id("surprise_id", &params.surprise_id)?;
    params.observed_state_row_key =
        normalize_required_text("observed_state_row_key", &params.observed_state_row_key)?;
    validate_unit_interval("threshold", params.threshold)?;
    if params.threshold <= 0.0 {
        return Err(params_error("threshold must be > 0.0"));
    }
    if params.stale_after_seconds == 0 {
        return Err(params_error("stale_after_seconds must be >= 1"));
    }
    if let Some(prediction) = params.prediction.as_mut() {
        normalize_prediction(prediction)?;
    }
    if let Some(observed) = params.observed_override.as_mut() {
        normalize_observed_override(observed)?;
    }
    if params.source_refs.len() > MAX_SOURCE_REFS {
        return Err(params_error(format!(
            "source_refs must contain <= {MAX_SOURCE_REFS} items"
        )));
    }
    Ok(params)
}

pub(super) fn source_refs_for_payload(
    params: &EverQuestSurpriseDetectParams,
    payload: &EverQuestSurprisePayload,
) -> Vec<EverQuestWorldModelSourceRef> {
    let mut refs = params.source_refs.clone();
    if let Some(prediction) = &payload.prediction {
        refs.extend(prediction.source_refs.clone());
    }
    refs.extend(payload.observed.source_refs.clone());
    if refs.is_empty() {
        refs.push(EverQuestWorldModelSourceRef {
            kind: "surprise_detector".to_owned(),
            row_key: None,
            path: None,
            start_offset: None,
            next_offset: None,
            content_sha256: None,
            summary: Some("detector persisted abstain evidence".to_owned()),
        });
    }
    if refs.len() > MAX_SOURCE_REFS {
        refs.truncate(MAX_SOURCE_REFS);
    }
    refs
}

fn normalize_prediction(prediction: &mut EverQuestSurprisePrediction) -> Result<(), ErrorData> {
    prediction.prediction_id =
        normalize_optional_id("prediction.prediction_id", prediction.prediction_id.take())?;
    prediction.expected_action = normalize_optional_text(
        "prediction.expected_action",
        prediction.expected_action.take(),
    )?;
    prediction.expected_zone_short_name = normalize_optional_id(
        "prediction.expected_zone_short_name",
        prediction.expected_zone_short_name.take(),
    )?;
    prediction.expected_outcome_kind = normalize_optional_id(
        "prediction.expected_outcome_kind",
        prediction.expected_outcome_kind.take(),
    )?;
    if prediction.expected_zone_short_name.is_none() && prediction.expected_outcome_kind.is_none() {
        return Err(params_error(
            "prediction must include expected_zone_short_name or expected_outcome_kind",
        ));
    }
    validate_unit_interval("prediction.confidence", prediction.confidence)?;
    Ok(())
}

fn normalize_observed_override(
    observed: &mut EverQuestSurpriseObservedOverride,
) -> Result<(), ErrorData> {
    observed.observed_outcome_id = normalize_optional_id(
        "observed_override.observed_outcome_id",
        observed.observed_outcome_id.take(),
    )?;
    observed.observed_zone_short_name = normalize_optional_id(
        "observed_override.observed_zone_short_name",
        observed.observed_zone_short_name.take(),
    )?;
    observed.observed_outcome_kind = normalize_optional_id(
        "observed_override.observed_outcome_kind",
        observed.observed_outcome_kind.take(),
    )?;
    observed.source_mode =
        normalize_optional_id("observed_override.source_mode", observed.source_mode.take())?;
    validate_unit_interval(
        "observed_override.zone_confidence",
        observed.zone_confidence,
    )?;
    validate_unit_interval(
        "observed_override.outcome_confidence",
        observed.outcome_confidence,
    )?;
    Ok(())
}

fn validate_profile_id(value: &str) -> Result<String, ErrorData> {
    let value = normalize_required_text("profile_id", value)?;
    if value != EVERQUEST_PROFILE_ID {
        return Err(params_error(format!(
            "profile_id must be {EVERQUEST_PROFILE_ID:?}"
        )));
    }
    Ok(value)
}

fn validate_id(field: &str, value: &str) -> Result<String, ErrorData> {
    let value = normalize_required_text(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(params_error(format!(
            "{field} may contain only ASCII letters, digits, '.', '_', and '-'"
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

fn normalize_optional_text(
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, ErrorData> {
    value
        .map(|value| normalize_required_text(field, &value))
        .transpose()
}

fn normalize_optional_id(field: &str, value: Option<String>) -> Result<Option<String>, ErrorData> {
    value.map(|value| validate_id(field, &value)).transpose()
}

fn validate_unit_interval(field: &str, value: f32) -> Result<(), ErrorData> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(params_error(format!("{field} must be between 0.0 and 1.0")));
    }
    Ok(())
}

pub(super) fn decode_json_row<T>(bytes: &[u8], label: &str) -> Result<T, ErrorData>
where
    T: DeserializeOwned,
{
    serde_json::from_slice(bytes).map_err(|error| {
        mcp_error(
            error_codes::STORAGE_CORRUPTED,
            format!("decode {label}: {error}"),
        )
    })
}

fn params_error(message: impl Into<String>) -> ErrorData {
    mcp_error(error_codes::TOOL_PARAMS_INVALID, message.into())
}
