use chrono::{DateTime, Utc};
use rmcp::{ErrorData, schemars::JsonSchema};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use synapse_core::error_codes;
use synapse_everquest::{
    EverQuestMapCoord, EverQuestZoneEdge, EverQuestZoneGraph, EverQuestZoneLandmark,
    build_zone_graph_from_root,
};

use super::{
    Json, Parameters, SynapseService,
    everquest_log::EVERQUEST_PROFILE_ID,
    everquest_state::{CURRENT_STATE_ROW_KEY, EverQuestCurrentState, EverQuestStateSource},
    tool, tool_router,
};
use crate::m1::mcp_error;

const TOOL: &str = "everquest_route_plan";
const SCHEMA_VERSION: u32 = 1;
const ROUTE_PLAN_PREFIX: &str = "everquest/route_plan/v1";
const DEFAULT_STALE_AFTER_SECONDS: u64 = 300;
const DEFAULT_MAX_WAYPOINTS: usize = 8;
const MAX_WAYPOINTS: usize = 16;
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 512;
const MAX_SOURCE_REFS: usize = 32;
const MIN_ROUTE_CONFIDENCE: f32 = 0.50;
const CALIBRATION_CONFLICT_DISTANCE: f64 = 150.0;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRoutePlanParams {
    pub plan_id: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_zone_short_name: Option<String>,
    #[serde(default = "default_state_row_key")]
    pub state_row_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_override: Option<EverQuestRouteStateOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_calibration: Option<EverQuestRouteMapCalibration>,
    #[serde(default = "default_stale_after_seconds")]
    pub stale_after_seconds: u64,
    #[serde(default = "default_max_waypoints")]
    pub max_waypoints: usize,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteStateOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<EverQuestRouteLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub source_refs: Vec<EverQuestRouteSourceRef>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteMapCalibration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<EverQuestRouteLocation>,
    #[serde(default = "default_full_confidence")]
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<EverQuestRouteSourceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRoutePlanResponse {
    pub ok: bool,
    pub row_key: String,
    pub stored_value_len_bytes: u64,
    pub plan: EverQuestRoutePlanRow,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRoutePlanRow {
    pub schema_version: u32,
    pub row_kind: String,
    pub profile_id: String,
    pub plan_id: String,
    pub row_key: String,
    pub generated_at: DateTime<Utc>,
    pub source_mode: String,
    pub source_state_row_key: String,
    pub target: EverQuestRouteTarget,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstain_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_zone_short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_location: Option<EverQuestRouteLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_landmark: Option<EverQuestRouteLandmark>,
    pub nearest_start_landmarks: Vec<EverQuestRouteLandmark>,
    pub nearest_target_landmarks: Vec<EverQuestRouteLandmark>,
    pub waypoints: Vec<EverQuestRouteWaypoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_distance: Option<f64>,
    pub confidence: f32,
    pub guard_requirements: Vec<String>,
    pub hazards: Vec<EverQuestRouteHazard>,
    pub source_refs: Vec<EverQuestRouteSourceRef>,
    pub evidence_boundary: EverQuestRouteEvidenceBoundary,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_short_name: Option<String>,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteLocation {
    pub map_x: f64,
    pub map_y: f64,
    pub map_z: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteLandmark {
    pub label: String,
    pub zone_short_name: String,
    pub map_x: f64,
    pub map_y: f64,
    pub map_z: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_from_current: Option<f64>,
    pub confidence: f32,
    pub source_path: String,
    pub source_line_number: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_zone_short_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteWaypoint {
    pub step_index: usize,
    pub waypoint_kind: String,
    pub label: String,
    pub zone_short_name: String,
    pub map_x: f64,
    pub map_y: f64,
    pub map_z: f64,
    pub distance_from_previous: f64,
    pub distance_from_start: f64,
    pub confidence: f32,
    pub guard_requirements: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_line_number: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteHazard {
    pub code: String,
    pub severity: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteSourceRef {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_number: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EverQuestRouteEvidenceBoundary {
    pub supports_planning: bool,
    pub movement_executed: bool,
    pub manual_fsv_required_for_runtime: bool,
    pub is_fsv: bool,
    pub redacted: bool,
    pub note: String,
}

#[derive(Clone, Debug)]
struct NormalizedParams {
    plan_id: String,
    profile_id: String,
    target_label: Option<String>,
    target_zone_short_name: Option<String>,
    state_row_key: String,
    state_override: Option<RouteStateOverride>,
    map_calibration: Option<RouteMapCalibration>,
    stale_after_seconds: u64,
    max_waypoints: usize,
    row_key: String,
}

#[derive(Clone, Debug)]
struct RouteStateOverride {
    zone_short_name: Option<String>,
    location: Option<EverQuestRouteLocation>,
    generated_at: Option<DateTime<Utc>>,
    confidence: f32,
    source_refs: Vec<EverQuestRouteSourceRef>,
}

#[derive(Clone, Debug)]
struct RouteMapCalibration {
    zone_short_name: Option<String>,
    location: Option<EverQuestRouteLocation>,
    confidence: f32,
    source_ref: Option<EverQuestRouteSourceRef>,
}

#[derive(Clone, Debug)]
struct RouteSourceState {
    source_mode: String,
    state_row_key: String,
    generated_at: Option<DateTime<Utc>>,
    zone_short_name: Option<String>,
    zone_confidence: f32,
    location: Option<EverQuestRouteLocation>,
    location_confidence: f32,
    source_refs: Vec<EverQuestRouteSourceRef>,
}

#[derive(Clone, Debug)]
struct RouteTargetMatch {
    landmark: EverQuestRouteLandmark,
    confidence: f32,
}

#[tool_router(router = everquest_route_tool_router, vis = "pub(super)")]
impl SynapseService {
    #[tool(
        description = "Plan and persist one bounded EverQuest route from current state to a local map landmark or zone line without executing movement"
    )]
    pub async fn everquest_route_plan(
        &self,
        params: Parameters<EverQuestRoutePlanParams>,
    ) -> Result<Json<EverQuestRoutePlanResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = TOOL,
            "tool.invocation kind=everquest_route_plan"
        );
        let normalized = normalize_params(params.0)?;
        let source_state = self.route_source_state(&normalized)?;
        let active = self.resolve_active_everquest_log().map_err(|detail| {
            mcp_error(
                error_codes::ACTION_TARGET_INVALID,
                format!("{TOOL} could not resolve active EverQuest install/log: {detail}"),
            )
        })?;
        let graph = build_zone_graph_from_root(&active.install_root).map_err(|error| {
            mcp_error(
                error_codes::STORAGE_READ_FAILED,
                format!("{TOOL} could not build EverQuest zone graph: {error}"),
            )
        })?;
        let row = route_plan_row(&normalized, &source_state, &graph);
        let (plan, stored_value_len_bytes) =
            self.persist_route_plan_json(&normalized.row_key, &row)?;
        Ok(Json(EverQuestRoutePlanResponse {
            ok: true,
            row_key: normalized.row_key,
            stored_value_len_bytes,
            plan,
        }))
    }
}

impl SynapseService {
    fn route_source_state(&self, params: &NormalizedParams) -> Result<RouteSourceState, ErrorData> {
        if let Some(override_state) = &params.state_override {
            return Ok(RouteSourceState {
                source_mode: "state_override".to_owned(),
                state_row_key: params.state_row_key.clone(),
                generated_at: override_state.generated_at,
                zone_short_name: override_state.zone_short_name.clone(),
                zone_confidence: override_state.confidence,
                location: override_state.location.clone(),
                location_confidence: override_state.confidence,
                source_refs: override_state.source_refs.clone(),
            });
        }
        let stored = {
            let runtime = self.reflex_runtime()?;
            let runtime = runtime.lock().map_err(|_| {
                mcp_error(
                    error_codes::TOOL_INTERNAL_ERROR,
                    "reflex runtime lock poisoned while reading EverQuest current-state row",
                )
            })?;
            runtime
                .storage_kv_row(params.state_row_key.as_bytes())
                .map_err(|error| mcp_error(error.code(), error.to_string()))?
        };
        let Some(stored) = stored else {
            return Ok(RouteSourceState {
                source_mode: "current_state_row_missing".to_owned(),
                state_row_key: params.state_row_key.clone(),
                generated_at: None,
                zone_short_name: None,
                zone_confidence: 0.0,
                location: None,
                location_confidence: 0.0,
                source_refs: vec![EverQuestRouteSourceRef {
                    kind: "synapse_storage_missing".to_owned(),
                    row_key: Some(params.state_row_key.clone()),
                    path: None,
                    line_number: None,
                    start_offset: None,
                    next_offset: None,
                    summary: Some("current-state row was absent before route planning".to_owned()),
                }],
            });
        };
        let state =
            decode_json_row::<EverQuestCurrentState>(&stored, "EverQuest current-state row")?;
        Ok(source_state_from_current_row(&params.state_row_key, &state))
    }

    fn persist_route_plan_json(
        &self,
        key: &str,
        row: &EverQuestRoutePlanRow,
    ) -> Result<(EverQuestRoutePlanRow, u64), ErrorData> {
        let encoded = serde_json::to_vec(row).map_err(|error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!("encode EverQuest route-plan row: {error}"),
            )
        })?;
        let stored = {
            let runtime = self.reflex_runtime()?;
            let runtime = runtime.lock().map_err(|_| {
                mcp_error(
                    error_codes::TOOL_INTERNAL_ERROR,
                    "reflex runtime lock poisoned while writing EverQuest route-plan row",
                )
            })?;
            runtime
                .storage_put_kv_rows(vec![(key.as_bytes().to_vec(), encoded)])
                .map_err(|error| {
                    mcp_error(
                        error_codes::STORAGE_WRITE_FAILED,
                        format!("write EverQuest route-plan row: {error}"),
                    )
                })?;
            runtime
                .storage_kv_row(key.as_bytes())
                .map_err(|error| {
                    mcp_error(
                        error_codes::STORAGE_READ_FAILED,
                        format!("read EverQuest route-plan row after write: {error}"),
                    )
                })?
                .ok_or_else(|| {
                    mcp_error(
                        error_codes::STORAGE_READ_FAILED,
                        "EverQuest route-plan row missing after write",
                    )
                })?
        };
        let readback =
            decode_json_row::<EverQuestRoutePlanRow>(&stored, "EverQuest route-plan row")?;
        Ok((readback, len_to_u64(stored.len())))
    }
}

fn normalize_params(params: EverQuestRoutePlanParams) -> Result<NormalizedParams, ErrorData> {
    let profile_id = validate_everquest_profile_id(&params.profile_id)?;
    let plan_id = validate_id("plan_id", &params.plan_id)?;
    let target_label = params
        .target_label
        .map(|value| normalize_required_text("target_label", &value))
        .transpose()?;
    let target_zone_short_name = params
        .target_zone_short_name
        .map(|value| validate_id("target_zone_short_name", &value))
        .transpose()?;
    if target_label.is_none() && target_zone_short_name.is_none() {
        return Err(params_error(
            "target_label or target_zone_short_name must be provided",
        ));
    }
    if params.stale_after_seconds == 0 {
        return Err(params_error("stale_after_seconds must be >= 1"));
    }
    if params.max_waypoints < 2 || params.max_waypoints > MAX_WAYPOINTS {
        return Err(params_error(format!(
            "max_waypoints must be between 2 and {MAX_WAYPOINTS}"
        )));
    }
    let state_row_key = normalize_required_text("state_row_key", &params.state_row_key)?;
    let state_override = params
        .state_override
        .map(normalize_state_override)
        .transpose()?;
    let map_calibration = params
        .map_calibration
        .map(normalize_map_calibration)
        .transpose()?;
    let row_key = route_plan_row_key(&profile_id, &plan_id);
    Ok(NormalizedParams {
        plan_id,
        profile_id,
        target_label,
        target_zone_short_name,
        state_row_key,
        state_override,
        map_calibration,
        stale_after_seconds: params.stale_after_seconds,
        max_waypoints: params.max_waypoints,
        row_key,
    })
}

fn normalize_state_override(
    override_state: EverQuestRouteStateOverride,
) -> Result<RouteStateOverride, ErrorData> {
    let zone_short_name = override_state
        .zone_short_name
        .map(|value| validate_id("state_override.zone_short_name", &value))
        .transpose()?;
    let confidence = override_state.confidence.unwrap_or(0.75);
    validate_unit_interval("state_override.confidence", confidence)?;
    let source_refs =
        normalize_source_refs("state_override.source_refs", override_state.source_refs)?;
    if source_refs.is_empty() {
        return Err(params_error(
            "state_override.source_refs must contain at least one physical SoT reference",
        ));
    }
    Ok(RouteStateOverride {
        zone_short_name,
        location: override_state.location,
        generated_at: override_state.generated_at,
        confidence,
        source_refs,
    })
}

fn normalize_map_calibration(
    calibration: EverQuestRouteMapCalibration,
) -> Result<RouteMapCalibration, ErrorData> {
    validate_unit_interval("map_calibration.confidence", calibration.confidence)?;
    let zone_short_name = calibration
        .zone_short_name
        .map(|value| validate_id("map_calibration.zone_short_name", &value))
        .transpose()?;
    let source_ref = calibration
        .source_ref
        .map(|source| normalize_source_ref("map_calibration.source_ref", source))
        .transpose()?;
    Ok(RouteMapCalibration {
        zone_short_name,
        location: calibration.location,
        confidence: calibration.confidence,
        source_ref,
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "route row assembly keeps abstain and ready-path fields in one auditable state transition"
)]
fn route_plan_row(
    params: &NormalizedParams,
    source: &RouteSourceState,
    graph: &EverQuestZoneGraph,
) -> EverQuestRoutePlanRow {
    let mut source_refs = source.source_refs.clone();
    let mut hazards = Vec::new();
    if let Some(calibration) = &params.map_calibration
        && let Some(ref source_ref) = calibration.source_ref
    {
        source_refs.push(source_ref.clone());
    }
    let target = EverQuestRouteTarget {
        label: params.target_label.clone(),
        zone_short_name: params.target_zone_short_name.clone(),
    };
    let guard_requirements = default_guard_requirements();
    let mut row = EverQuestRoutePlanRow {
        schema_version: SCHEMA_VERSION,
        row_kind: "everquest_route_plan".to_owned(),
        profile_id: params.profile_id.clone(),
        plan_id: params.plan_id.clone(),
        row_key: params.row_key.clone(),
        generated_at: Utc::now(),
        source_mode: source.source_mode.clone(),
        source_state_row_key: source.state_row_key.clone(),
        target,
        decision: "abstain_uninitialized".to_owned(),
        abstain_reason: None,
        current_zone_short_name: source.zone_short_name.clone(),
        current_location: source.location.clone(),
        target_landmark: None,
        nearest_start_landmarks: Vec::new(),
        nearest_target_landmarks: Vec::new(),
        waypoints: Vec::new(),
        total_distance: None,
        confidence: 0.0,
        guard_requirements,
        hazards: Vec::new(),
        source_refs,
        evidence_boundary: evidence_boundary(),
    };

    if source.source_mode == "current_state_row_missing" {
        abstain(
            &mut row,
            "abstain_current_state_missing",
            "current-state row is absent",
        );
        return row;
    }
    if is_stale_source(source.generated_at, params.stale_after_seconds) {
        hazards.push(EverQuestRouteHazard {
            code: "stale_current_state".to_owned(),
            severity: "warning".to_owned(),
            detail: format!(
                "current state is older than {} seconds",
                params.stale_after_seconds
            ),
        });
        row.hazards = hazards;
        abstain(
            &mut row,
            "abstain_stale_current_state",
            "current state is stale; refresh /loc/current_state before planning",
        );
        return row;
    }
    let Some(zone_short_name) = source.zone_short_name.as_deref() else {
        abstain(
            &mut row,
            "abstain_unknown_current_zone",
            "current zone short name is unknown",
        );
        return row;
    };
    let Some(current_location) = source.location.as_ref() else {
        abstain(
            &mut row,
            "abstain_no_current_loc",
            "current /loc map coordinate is unknown",
        );
        return row;
    };
    if let Some(conflict) = calibration_conflict(source, params.map_calibration.as_ref()) {
        row.hazards.push(conflict.clone());
        abstain(
            &mut row,
            "abstain_conflicting_map_calibration",
            conflict.detail,
        );
        return row;
    }
    let current_coord = route_location_to_coord(current_location);
    row.nearest_start_landmarks = graph
        .nearest_landmarks(zone_short_name, &current_coord, 3)
        .into_iter()
        .map(|nearest| {
            landmark_from_zone_landmark(&nearest.landmark, Some(nearest.distance), 0.80, None)
        })
        .collect();

    let Some(target_match) = find_target(params, graph, zone_short_name, current_location) else {
        abstain(
            &mut row,
            "abstain_target_not_found",
            target_not_found_reason(params, zone_short_name),
        );
        return row;
    };

    let target_coord = EverQuestMapCoord {
        x: target_match.landmark.map_x,
        y: target_match.landmark.map_y,
        z: target_match.landmark.map_z,
    };
    row.nearest_target_landmarks = graph
        .nearest_landmarks(zone_short_name, &target_coord, 3)
        .into_iter()
        .map(|nearest| {
            landmark_from_zone_landmark(&nearest.landmark, Some(nearest.distance), 0.80, None)
        })
        .collect();
    let distance_to_target = distance(&current_coord, &target_coord);
    let confidence =
        (source.zone_confidence * source.location_confidence * target_match.confidence)
            .clamp(0.0, 1.0);
    row.target_landmark = Some(target_match.landmark.clone());
    row.total_distance = Some(distance_to_target);
    row.confidence = confidence;
    row.waypoints = vec![
        EverQuestRouteWaypoint {
            step_index: 0,
            waypoint_kind: "current_state".to_owned(),
            label: "current_location".to_owned(),
            zone_short_name: zone_short_name.to_owned(),
            map_x: current_location.map_x,
            map_y: current_location.map_y,
            map_z: current_location.map_z,
            distance_from_previous: 0.0,
            distance_from_start: 0.0,
            confidence: (source.zone_confidence * source.location_confidence).clamp(0.0, 1.0),
            guard_requirements: vec!["verify_loc_before_step".to_owned()],
            source_path: None,
            source_line_number: None,
        },
        EverQuestRouteWaypoint {
            step_index: 1,
            waypoint_kind: "target_landmark".to_owned(),
            label: target_match.landmark.label.clone(),
            zone_short_name: target_match.landmark.zone_short_name.clone(),
            map_x: target_match.landmark.map_x,
            map_y: target_match.landmark.map_y,
            map_z: target_match.landmark.map_z,
            distance_from_previous: distance_to_target,
            distance_from_start: distance_to_target,
            confidence: target_match.confidence,
            guard_requirements: vec![
                "bounded_step_probe".to_owned(),
                "replan_after_zone_change_or_surprise".to_owned(),
            ],
            source_path: Some(target_match.landmark.source_path.clone()),
            source_line_number: Some(target_match.landmark.source_line_number),
        },
    ];
    row.waypoints.truncate(params.max_waypoints);
    if confidence >= MIN_ROUTE_CONFIDENCE {
        "route_ready".clone_into(&mut row.decision);
    } else {
        abstain(
            &mut row,
            "abstain_low_confidence",
            format!(
                "route confidence {confidence:.3} is below active threshold {MIN_ROUTE_CONFIDENCE:.2}"
            ),
        );
    }
    row
}

fn source_state_from_current_row(row_key: &str, state: &EverQuestCurrentState) -> RouteSourceState {
    let location = state
        .location
        .value
        .as_ref()
        .map(|location| EverQuestRouteLocation {
            map_x: location.map_x,
            map_y: location.map_y,
            map_z: location.map_z,
        });
    let mut source_refs = vec![EverQuestRouteSourceRef {
        kind: "synapse_storage".to_owned(),
        row_key: Some(row_key.to_owned()),
        path: None,
        line_number: None,
        start_offset: None,
        next_offset: None,
        summary: Some("persisted EverQuest current-state row".to_owned()),
    }];
    source_refs.extend(state_source_refs(&state.zone_short_name.sources));
    source_refs.extend(state_source_refs(&state.location.sources));
    source_refs.truncate(MAX_SOURCE_REFS);
    RouteSourceState {
        source_mode: "current_state_row".to_owned(),
        state_row_key: row_key.to_owned(),
        generated_at: Some(state.generated_at),
        zone_short_name: state.zone_short_name.value.clone(),
        zone_confidence: state.zone_short_name.confidence,
        location,
        location_confidence: state.location.confidence,
        source_refs,
    }
}

fn state_source_refs(sources: &[EverQuestStateSource]) -> Vec<EverQuestRouteSourceRef> {
    sources
        .iter()
        .map(|source| EverQuestRouteSourceRef {
            kind: source.kind.clone(),
            row_key: None,
            path: source.path.clone(),
            line_number: None,
            start_offset: source.start_offset,
            next_offset: source.next_offset,
            summary: source.summary.clone(),
        })
        .collect()
}

fn find_target(
    params: &NormalizedParams,
    graph: &EverQuestZoneGraph,
    current_zone: &str,
    current_location: &EverQuestRouteLocation,
) -> Option<RouteTargetMatch> {
    if let Some(label) = params.target_label.as_deref() {
        if let Some(edge) =
            best_matching_edge_by_label(params, graph, current_zone, label, current_location)
        {
            return Some(target_from_edge(&edge, current_location));
        }
        if let Some(landmark) =
            best_matching_landmark_by_label(graph, current_zone, label, current_location)
        {
            return Some(RouteTargetMatch {
                landmark: landmark_from_zone_landmark(
                    &landmark,
                    Some(distance(
                        &route_location_to_coord(current_location),
                        &landmark.location,
                    )),
                    0.75,
                    None,
                ),
                confidence: 0.75,
            });
        }
    }
    let target_zone = params.target_zone_short_name.as_deref()?;
    graph
        .exits_for_zone(current_zone)
        .into_iter()
        .filter(|edge| {
            edge.target_zone_short_name
                .as_deref()
                .is_some_and(|zone| zone.eq_ignore_ascii_case(target_zone))
        })
        .min_by(|left, right| {
            let current = route_location_to_coord(current_location);
            distance(&left.location, &current).total_cmp(&distance(&right.location, &current))
        })
        .map(|edge| target_from_edge(&edge, current_location))
}

fn best_matching_edge_by_label(
    params: &NormalizedParams,
    graph: &EverQuestZoneGraph,
    current_zone: &str,
    label: &str,
    current_location: &EverQuestRouteLocation,
) -> Option<EverQuestZoneEdge> {
    let normalized_label = normalize_label(label);
    graph
        .exits_for_zone(current_zone)
        .into_iter()
        .filter(|edge| normalize_label(&edge.label) == normalized_label)
        .filter(|edge| {
            params
                .target_zone_short_name
                .as_deref()
                .is_none_or(|target| {
                    edge.target_zone_short_name
                        .as_deref()
                        .is_none_or(|zone| zone.eq_ignore_ascii_case(target))
                })
        })
        .min_by(|left, right| {
            let current = route_location_to_coord(current_location);
            distance(&left.location, &current).total_cmp(&distance(&right.location, &current))
        })
}

fn best_matching_landmark_by_label(
    graph: &EverQuestZoneGraph,
    current_zone: &str,
    label: &str,
    current_location: &EverQuestRouteLocation,
) -> Option<EverQuestZoneLandmark> {
    let normalized_label = normalize_label(label);
    graph
        .landmarks_for_zone(current_zone)
        .into_iter()
        .filter(|landmark| landmark.normalized_label == normalized_label)
        .min_by(|left, right| {
            let current = route_location_to_coord(current_location);
            distance(&left.location, &current).total_cmp(&distance(&right.location, &current))
        })
}

fn target_from_edge(
    edge: &EverQuestZoneEdge,
    current_location: &EverQuestRouteLocation,
) -> RouteTargetMatch {
    RouteTargetMatch {
        landmark: EverQuestRouteLandmark {
            label: edge.label.clone(),
            zone_short_name: edge.source_zone_short_name.clone(),
            map_x: edge.location.x,
            map_y: edge.location.y,
            map_z: edge.location.z,
            distance_from_current: Some(distance(
                &route_location_to_coord(current_location),
                &edge.location,
            )),
            confidence: edge.confidence,
            source_path: edge.source_path.display().to_string(),
            source_line_number: edge.source_line_number,
            target_zone_short_name: edge.target_zone_short_name.clone(),
        },
        confidence: edge.confidence,
    }
}

fn landmark_from_zone_landmark(
    landmark: &EverQuestZoneLandmark,
    distance_from_current: Option<f64>,
    confidence: f32,
    target_zone_short_name: Option<String>,
) -> EverQuestRouteLandmark {
    EverQuestRouteLandmark {
        label: landmark.label.clone(),
        zone_short_name: landmark.zone_short_name.clone(),
        map_x: landmark.location.x,
        map_y: landmark.location.y,
        map_z: landmark.location.z,
        distance_from_current,
        confidence,
        source_path: landmark.source_path.display().to_string(),
        source_line_number: landmark.source_line_number,
        target_zone_short_name,
    }
}

fn calibration_conflict(
    source: &RouteSourceState,
    calibration: Option<&RouteMapCalibration>,
) -> Option<EverQuestRouteHazard> {
    let calibration = calibration?;
    if calibration.confidence < MIN_ROUTE_CONFIDENCE {
        return None;
    }
    if let (Some(source_zone), Some(calibration_zone)) = (
        source.zone_short_name.as_deref(),
        calibration.zone_short_name.as_deref(),
    ) && !source_zone.eq_ignore_ascii_case(calibration_zone)
    {
        return Some(EverQuestRouteHazard {
            code: "map_calibration_zone_conflict".to_owned(),
            severity: "high".to_owned(),
            detail: format!(
                "current-state zone {source_zone:?} conflicts with calibrated map zone {calibration_zone:?}"
            ),
        });
    }
    if let (Some(source_location), Some(calibration_location)) =
        (source.location.as_ref(), calibration.location.as_ref())
    {
        let delta = distance(
            &route_location_to_coord(source_location),
            &route_location_to_coord(calibration_location),
        );
        if delta > CALIBRATION_CONFLICT_DISTANCE {
            return Some(EverQuestRouteHazard {
                code: "map_calibration_location_conflict".to_owned(),
                severity: "high".to_owned(),
                detail: format!(
                    "current-state location differs from map calibration by {delta:.2} map units"
                ),
            });
        }
    }
    None
}

fn target_not_found_reason(params: &NormalizedParams, current_zone: &str) -> String {
    match (
        params.target_label.as_deref(),
        params.target_zone_short_name.as_deref(),
    ) {
        (Some(label), Some(zone)) => format!(
            "target label {label:?} with zone {zone:?} was not found in current zone {current_zone:?}"
        ),
        (Some(label), None) => {
            format!("target label {label:?} was not found in current zone {current_zone:?}")
        }
        (None, Some(zone)) => format!(
            "no zone-line target for zone {zone:?} was found in current zone {current_zone:?}"
        ),
        (None, None) => "target is absent".to_owned(),
    }
}

fn abstain(
    row: &mut EverQuestRoutePlanRow,
    decision: impl Into<String>,
    reason: impl Into<String>,
) {
    row.decision = decision.into();
    row.abstain_reason = Some(reason.into());
    row.confidence = 0.0;
    row.waypoints.clear();
    row.total_distance = None;
}

fn default_guard_requirements() -> Vec<String> {
    vec![
        "verify_everquest_foreground".to_owned(),
        "verify_world_focus_not_chat".to_owned(),
        "verify_loc_before_step".to_owned(),
        "bounded_step_probe_only".to_owned(),
        "replan_after_surprise_zone_change_or_stale_loc".to_owned(),
    ]
}

fn evidence_boundary() -> EverQuestRouteEvidenceBoundary {
    EverQuestRouteEvidenceBoundary {
        supports_planning: true,
        movement_executed: false,
        manual_fsv_required_for_runtime: true,
        is_fsv: false,
        redacted: true,
        note: "Route plans are bounded planning rows only; movement requires separate attended action FSV and storage/UI/log readback."
            .to_owned(),
    }
}

const fn route_location_to_coord(location: &EverQuestRouteLocation) -> EverQuestMapCoord {
    EverQuestMapCoord {
        x: location.map_x,
        y: location.map_y,
        z: location.map_z,
    }
}

fn distance(left: &EverQuestMapCoord, right: &EverQuestMapCoord) -> f64 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    let dz = left.z - right.z;
    dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt()
}

fn is_stale_source(source_generated_at: Option<DateTime<Utc>>, stale_after_seconds: u64) -> bool {
    source_generated_at.is_some_and(|generated_at| {
        let age = Utc::now().signed_duration_since(generated_at);
        age.num_seconds() > i64::try_from(stale_after_seconds).unwrap_or(i64::MAX)
    })
}

fn normalize_source_refs(
    field: &str,
    refs: Vec<EverQuestRouteSourceRef>,
) -> Result<Vec<EverQuestRouteSourceRef>, ErrorData> {
    if refs.len() > MAX_SOURCE_REFS {
        return Err(params_error(format!(
            "{field} must contain <= {MAX_SOURCE_REFS} refs"
        )));
    }
    refs.into_iter()
        .enumerate()
        .map(|(index, source)| normalize_source_ref(&format!("{field}[{index}]"), source))
        .collect()
}

fn normalize_source_ref(
    field: &str,
    source: EverQuestRouteSourceRef,
) -> Result<EverQuestRouteSourceRef, ErrorData> {
    Ok(EverQuestRouteSourceRef {
        kind: normalize_required_text(&format!("{field}.kind"), &source.kind)?,
        row_key: source
            .row_key
            .map(|value| normalize_required_text(&format!("{field}.row_key"), &value))
            .transpose()?,
        path: source
            .path
            .map(|value| normalize_required_text(&format!("{field}.path"), &value))
            .transpose()?,
        line_number: source.line_number,
        start_offset: source.start_offset,
        next_offset: source.next_offset,
        summary: source
            .summary
            .map(|value| normalize_required_text(&format!("{field}.summary"), &value))
            .transpose()?,
    })
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

fn normalize_label(label: &str) -> String {
    label
        .chars()
        .flat_map(char::to_lowercase)
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

fn route_plan_row_key(profile_id: &str, plan_id: &str) -> String {
    format!("{ROUTE_PLAN_PREFIX}/{profile_id}/{plan_id}")
}

fn default_profile_id() -> String {
    EVERQUEST_PROFILE_ID.to_owned()
}

fn default_state_row_key() -> String {
    CURRENT_STATE_ROW_KEY.to_owned()
}

const fn default_stale_after_seconds() -> u64 {
    DEFAULT_STALE_AFTER_SECONDS
}

const fn default_max_waypoints() -> usize {
    DEFAULT_MAX_WAYPOINTS
}

const fn default_full_confidence() -> f32 {
    1.0
}

fn len_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn params_error(message: impl Into<String>) -> ErrorData {
    mcp_error(error_codes::TOOL_PARAMS_INVALID, message.into())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use synapse_everquest::{
        EverQuestMapCoord, EverQuestZoneEdge, EverQuestZoneEdgeResolution, EverQuestZoneGraph,
        EverQuestZoneLandmark, EverQuestZoneNode,
    };

    use super::*;

    #[test]
    fn route_ready_to_matching_zone_line() {
        let params = params("happy", Some("to_Nektulos_Forest"), Some("nektulos"));
        let source = source_state(Some("neriaka"), Some(location(154.0, 50.94, 31.19)));
        let row = route_plan_row(&params, &source, &graph());

        assert_eq!(row.decision, "route_ready");
        assert_eq!(
            row.target_landmark.as_ref().unwrap().source_line_number,
            2983
        );
        assert_eq!(row.waypoints.len(), 2);
        assert!(row.confidence > 0.7);
        assert!(!row.evidence_boundary.movement_executed);
    }

    #[test]
    fn unknown_zone_abstains_with_persistable_row() {
        let params = params("unknown-zone", Some("to_Nektulos_Forest"), Some("nektulos"));
        let source = source_state(None, Some(location(154.0, 50.94, 31.19)));
        let row = route_plan_row(&params, &source, &graph());

        assert_eq!(row.decision, "abstain_unknown_current_zone");
        assert!(row.waypoints.is_empty());
        assert_eq!(row.current_location.unwrap().map_x, 154.0);
    }

    #[test]
    fn missing_location_abstains() {
        let params = params("no-loc", Some("to_Nektulos_Forest"), Some("nektulos"));
        let source = source_state(Some("neriaka"), None);
        let row = route_plan_row(&params, &source, &graph());

        assert_eq!(row.decision, "abstain_no_current_loc");
        assert!(row.total_distance.is_none());
    }

    #[test]
    fn absent_target_abstains() {
        let params = params("absent", Some("not_on_this_map"), None);
        let source = source_state(Some("neriaka"), Some(location(154.0, 50.94, 31.19)));
        let row = route_plan_row(&params, &source, &graph());

        assert_eq!(row.decision, "abstain_target_not_found");
        assert!(row.abstain_reason.unwrap().contains("not_on_this_map"));
    }

    #[test]
    fn conflicting_calibration_abstains() {
        let mut params = params("conflict", Some("to_Nektulos_Forest"), Some("nektulos"));
        params.map_calibration = Some(RouteMapCalibration {
            zone_short_name: Some("nektulos".to_owned()),
            location: None,
            confidence: 0.9,
            source_ref: None,
        });
        let source = source_state(Some("neriaka"), Some(location(154.0, 50.94, 31.19)));
        let row = route_plan_row(&params, &source, &graph());

        assert_eq!(row.decision, "abstain_conflicting_map_calibration");
        assert_eq!(row.hazards[0].code, "map_calibration_zone_conflict");
    }

    fn params(
        plan_id: &str,
        target_label: Option<&str>,
        target_zone_short_name: Option<&str>,
    ) -> NormalizedParams {
        NormalizedParams {
            plan_id: plan_id.to_owned(),
            profile_id: EVERQUEST_PROFILE_ID.to_owned(),
            target_label: target_label.map(str::to_owned),
            target_zone_short_name: target_zone_short_name.map(str::to_owned),
            state_row_key: CURRENT_STATE_ROW_KEY.to_owned(),
            state_override: None,
            map_calibration: None,
            stale_after_seconds: DEFAULT_STALE_AFTER_SECONDS,
            max_waypoints: DEFAULT_MAX_WAYPOINTS,
            row_key: route_plan_row_key(EVERQUEST_PROFILE_ID, plan_id),
        }
    }

    fn source_state(
        zone_short_name: Option<&str>,
        location: Option<EverQuestRouteLocation>,
    ) -> RouteSourceState {
        RouteSourceState {
            source_mode: "state_override".to_owned(),
            state_row_key: CURRENT_STATE_ROW_KEY.to_owned(),
            generated_at: Some(Utc::now()),
            zone_short_name: zone_short_name.map(str::to_owned),
            zone_confidence: if zone_short_name.is_some() { 0.95 } else { 0.0 },
            location,
            location_confidence: 0.98,
            source_refs: vec![EverQuestRouteSourceRef {
                kind: "unit_test".to_owned(),
                row_key: None,
                path: None,
                line_number: None,
                start_offset: None,
                next_offset: None,
                summary: Some("synthetic route source".to_owned()),
            }],
        }
    }

    fn location(map_x: f64, map_y: f64, map_z: f64) -> EverQuestRouteLocation {
        EverQuestRouteLocation {
            map_x,
            map_y,
            map_z,
        }
    }

    fn graph() -> EverQuestZoneGraph {
        EverQuestZoneGraph {
            nodes: vec![
                EverQuestZoneNode {
                    zone_short_name: "neriaka".to_owned(),
                    display_name: Some("Neriak - Foreign Quarter".to_owned()),
                    source_path: PathBuf::from("neriaka.txt"),
                    len_bytes: 1,
                    last_modified_unix_ms: None,
                },
                EverQuestZoneNode {
                    zone_short_name: "nektulos".to_owned(),
                    display_name: Some("Nektulos Forest".to_owned()),
                    source_path: PathBuf::from("nektulos.txt"),
                    len_bytes: 1,
                    last_modified_unix_ms: None,
                },
            ],
            landmarks: vec![
                EverQuestZoneLandmark {
                    zone_short_name: "neriaka".to_owned(),
                    label: "Priest_of_Discord".to_owned(),
                    normalized_label: normalize_label("Priest_of_Discord"),
                    location: EverQuestMapCoord {
                        x: 160.0,
                        y: 55.0,
                        z: 31.0,
                    },
                    layer: 3,
                    source_path: PathBuf::from("neriaka.txt"),
                    source_line_number: 2970,
                },
                EverQuestZoneLandmark {
                    zone_short_name: "neriaka".to_owned(),
                    label: "to_Nektulos_Forest".to_owned(),
                    normalized_label: normalize_label("to_Nektulos_Forest"),
                    location: EverQuestMapCoord {
                        x: -155.1781,
                        y: -20.6847,
                        z: 28.6260,
                    },
                    layer: 3,
                    source_path: PathBuf::from("neriaka.txt"),
                    source_line_number: 2983,
                },
            ],
            edges: vec![EverQuestZoneEdge {
                source_zone_short_name: "neriaka".to_owned(),
                target_zone_short_name: Some("nektulos".to_owned()),
                target_display_name: Some("Nektulos Forest".to_owned()),
                target_hint: "Nektulos_Forest".to_owned(),
                normalized_target_hint: normalize_label("Nektulos_Forest"),
                label: "to_Nektulos_Forest".to_owned(),
                location: EverQuestMapCoord {
                    x: -155.1781,
                    y: -20.6847,
                    z: 28.6260,
                },
                confidence: 0.85,
                resolution: EverQuestZoneEdgeResolution::Alias,
                source_path: PathBuf::from("neriaka.txt"),
                source_line_number: 2983,
            }],
            unresolved_edge_count: 0,
            skipped_maps: Vec::new(),
        }
    }
}
