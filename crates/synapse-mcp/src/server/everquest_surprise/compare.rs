use chrono::{DateTime, Utc};

use super::model::{
    EverQuestSurpriseComparison, EverQuestSurpriseDetectParams, EverQuestSurpriseObserved,
    EverQuestSurprisePrediction,
};
use crate::server::{
    everquest_state::EverQuestCurrentState,
    everquest_world_model::model::EverQuestWorldModelSourceRef,
};

pub(super) fn compare_prediction(
    prediction: Option<&EverQuestSurprisePrediction>,
    observed: &EverQuestSurpriseObserved,
    params: &EverQuestSurpriseDetectParams,
) -> EverQuestSurpriseComparison {
    let Some(prediction) = prediction else {
        return comparison(
            "abstain_missing_prediction",
            false,
            true,
            0.0,
            params.threshold,
            Vec::new(),
            vec!["prediction_missing".to_owned()],
        );
    };
    if prediction.confidence < params.threshold {
        return comparison(
            "abstain_low_confidence_prediction",
            false,
            true,
            0.0,
            params.threshold,
            Vec::new(),
            vec!["prediction_confidence_below_threshold".to_owned()],
        );
    }
    if observation_is_stale(observed.observed_at, params.stale_after_seconds) {
        return comparison(
            "abstain_stale_observation",
            false,
            true,
            0.0,
            params.threshold,
            Vec::new(),
            vec!["observed_state_or_log_cursor_stale".to_owned()],
        );
    }
    if observed.zone_confidence < params.threshold || observed.outcome_confidence < params.threshold
    {
        return comparison(
            "abstain_low_confidence_observation",
            false,
            true,
            0.0,
            params.threshold,
            Vec::new(),
            vec!["observed_zone_or_outcome_confidence_below_threshold".to_owned()],
        );
    }

    let mut compared_fields = Vec::new();
    let mut mismatch_reasons = Vec::new();
    compare_text(
        "zone_short_name",
        prediction.expected_zone_short_name.as_deref(),
        observed.observed_zone_short_name.as_deref(),
        &mut compared_fields,
        &mut mismatch_reasons,
    );
    compare_text(
        "outcome_kind",
        prediction.expected_outcome_kind.as_deref(),
        observed.observed_outcome_kind.as_deref(),
        &mut compared_fields,
        &mut mismatch_reasons,
    );
    if compared_fields.is_empty() {
        return comparison(
            "abstain_no_comparable_fields",
            false,
            true,
            0.0,
            params.threshold,
            compared_fields,
            vec!["prediction_and_observation_share_no_comparable_fields".to_owned()],
        );
    }
    let divergence_score = divergence_score(mismatch_reasons.len(), compared_fields.len());
    let surprise_detected = divergence_score >= params.threshold;
    comparison(
        if surprise_detected {
            "surprise_detected"
        } else {
            "expected_outcome_confirmed"
        },
        surprise_detected,
        surprise_detected,
        divergence_score,
        params.threshold,
        compared_fields,
        mismatch_reasons,
    )
}

pub(super) fn observed_from_current_state(
    row_key: &str,
    state: &EverQuestCurrentState,
) -> EverQuestSurpriseObserved {
    let mut source_refs = vec![EverQuestWorldModelSourceRef {
        kind: "current_state_row".to_owned(),
        row_key: Some(row_key.to_owned()),
        path: None,
        start_offset: None,
        next_offset: None,
        content_sha256: None,
        summary: Some("observed state read from Synapse storage".to_owned()),
    }];
    source_refs.push(EverQuestWorldModelSourceRef {
        kind: "eq_log_cursor".to_owned(),
        row_key: None,
        path: Some(state.log_cursor.path.clone()),
        start_offset: Some(state.log_cursor.start_offset),
        next_offset: Some(state.log_cursor.next_offset),
        content_sha256: None,
        summary: Some("current-state log cursor".to_owned()),
    });
    for source in &state.zone_short_name.sources {
        source_refs.push(EverQuestWorldModelSourceRef {
            kind: format!("state_zone_{}", source.kind),
            row_key: None,
            path: source.path.clone(),
            start_offset: source.start_offset,
            next_offset: source.next_offset,
            content_sha256: None,
            summary: source.summary.clone(),
        });
    }
    EverQuestSurpriseObserved {
        source_mode: "current_state_row".to_owned(),
        observed_state_row_key: row_key.to_owned(),
        observed_outcome_id: None,
        observed_zone_short_name: state.zone_short_name.value.clone(),
        observed_outcome_kind: state
            .zone_short_name
            .value
            .as_ref()
            .map(|_| "state_estimate".to_owned()),
        observed_at: Some(state.generated_at),
        zone_confidence: state.zone_short_name.confidence,
        outcome_confidence: state.zone_short_name.confidence,
        source_refs,
    }
}

pub(super) fn remediation_for(comparison: &EverQuestSurpriseComparison) -> Vec<String> {
    if comparison.stop_condition {
        vec![
            "stop_gameplay_actions".to_owned(),
            "reestimate_current_state".to_owned(),
            "repair_world_model_from_physical_sot".to_owned(),
        ]
    } else {
        vec!["continue_supervised_planning".to_owned()]
    }
}

fn compare_text(
    field: &str,
    expected: Option<&str>,
    observed: Option<&str>,
    compared_fields: &mut Vec<String>,
    mismatch_reasons: &mut Vec<String>,
) {
    if let (Some(expected), Some(observed)) = (expected, observed) {
        compared_fields.push(field.to_owned());
        if !expected.eq_ignore_ascii_case(observed) {
            mismatch_reasons.push(format!("{field}_mismatch"));
        }
    }
}

fn comparison(
    decision: &str,
    surprise_detected: bool,
    stop_condition: bool,
    divergence_score: f32,
    threshold: f32,
    compared_fields: Vec<String>,
    mismatch_reasons: Vec<String>,
) -> EverQuestSurpriseComparison {
    EverQuestSurpriseComparison {
        decision: decision.to_owned(),
        surprise_detected,
        stop_condition,
        divergence_score,
        threshold,
        compared_fields,
        mismatch_reasons,
    }
}

fn divergence_score(mismatch_count: usize, compared_count: usize) -> f32 {
    let mismatches = u16::try_from(mismatch_count).unwrap_or(u16::MAX);
    let compared = u16::try_from(compared_count).unwrap_or(u16::MAX).max(1);
    f32::from(mismatches) / f32::from(compared)
}

fn observation_is_stale(observed_at: Option<DateTime<Utc>>, stale_after_seconds: u64) -> bool {
    let Some(observed_at) = observed_at else {
        return false;
    };
    Utc::now().signed_duration_since(observed_at).num_seconds()
        > i64::try_from(stale_after_seconds).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{
        everquest_log::EVERQUEST_PROFILE_ID,
        everquest_state::CURRENT_STATE_ROW_KEY,
        everquest_surprise::model::{DEFAULT_STALE_AFTER_SECONDS, DEFAULT_THRESHOLD},
    };

    fn prediction(zone: &str, outcome: &str, confidence: f32) -> EverQuestSurprisePrediction {
        EverQuestSurprisePrediction {
            prediction_id: Some("pred1".to_owned()),
            expected_action: Some("bounded_move".to_owned()),
            expected_zone_short_name: Some(zone.to_owned()),
            expected_outcome_kind: Some(outcome.to_owned()),
            confidence,
            source_refs: Vec::new(),
        }
    }

    fn observed(zone: &str, outcome: &str, zone_confidence: f32) -> EverQuestSurpriseObserved {
        EverQuestSurpriseObserved {
            source_mode: "unit_test".to_owned(),
            observed_state_row_key: CURRENT_STATE_ROW_KEY.to_owned(),
            observed_outcome_id: Some("actual1".to_owned()),
            observed_zone_short_name: Some(zone.to_owned()),
            observed_outcome_kind: Some(outcome.to_owned()),
            observed_at: Some(Utc::now()),
            zone_confidence,
            outcome_confidence: zone_confidence,
            source_refs: Vec::new(),
        }
    }

    fn params(prediction: Option<EverQuestSurprisePrediction>) -> EverQuestSurpriseDetectParams {
        EverQuestSurpriseDetectParams {
            surprise_id: "unit".to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            prediction,
            observed_state_row_key: CURRENT_STATE_ROW_KEY.to_owned(),
            observed_override: None,
            threshold: DEFAULT_THRESHOLD,
            stale_after_seconds: DEFAULT_STALE_AFTER_SECONDS,
            source_refs: Vec::new(),
        }
    }

    #[test]
    fn detects_unexpected_zone_change() {
        let params = params(Some(prediction("neriaka", "no_zone_change", 0.9)));
        let row = compare_prediction(
            params.prediction.as_ref(),
            &observed("nektulos", "zone_entered", 0.95),
            &params,
        );

        assert_eq!(row.decision, "surprise_detected");
        assert!(row.surprise_detected);
        assert!(row.stop_condition);
        assert_eq!(row.mismatch_reasons.len(), 2);
    }

    #[test]
    fn confirms_expected_outcome() {
        let params = params(Some(prediction("nektulos", "zone_entered", 0.9)));
        let row = compare_prediction(
            params.prediction.as_ref(),
            &observed("nektulos", "zone_entered", 0.95),
            &params,
        );

        assert_eq!(row.decision, "expected_outcome_confirmed");
        assert!(!row.surprise_detected);
        assert!(!row.stop_condition);
    }

    #[test]
    fn missing_prediction_stops_for_repair() {
        let params = params(None);
        let row = compare_prediction(None, &observed("nektulos", "zone_entered", 0.95), &params);

        assert_eq!(row.decision, "abstain_missing_prediction");
        assert!(row.stop_condition);
        assert!(!row.surprise_detected);
    }

    #[test]
    fn low_confidence_observation_abstains() {
        let params = params(Some(prediction("neriaka", "no_zone_change", 0.9)));
        let row = compare_prediction(
            params.prediction.as_ref(),
            &observed("nektulos", "zone_entered", 0.2),
            &params,
        );

        assert_eq!(row.decision, "abstain_low_confidence_observation");
        assert!(row.stop_condition);
    }
}
