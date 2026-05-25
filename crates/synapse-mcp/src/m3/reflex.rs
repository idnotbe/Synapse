use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use rmcp::ErrorData;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use synapse_core::{
    Action, AimTarget, Backend, ButtonAction, ComboInput, ComboStep, DataPredicate, EventFilter,
    Key, KeyCode, ReflexAimAxis, ReflexButtonTarget, ReflexLifetime, ReflexStatus, ReflexThen,
    StoredReflexAudit, error_codes, new_reflex_id,
};
use synapse_reflex::{
    AimTrackParams, AimTrackTarget, HoldButtonParams, HoldMoveParams, ReflexCancelOutcome,
    ReflexError, ReflexRuntime, ScheduledReflex,
};

use crate::{
    m1::mcp_error,
    m2::{ActPressParams, ActTypeParams, action_from_press_params, action_from_type_params},
};

use super::{
    M3ToolStub,
    permissions::{Permission, RequiredPermissions, add_action_permissions, required},
};

fn reflex_kind_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "string",
        "enum": ["aim_track", "hold_move", "hold_button", "combo", "on_event"]
    })
}

const fn default_reflex_priority() -> u32 {
    synapse_reflex::DEFAULT_REFLEX_PRIORITY
}

const fn default_lifetime() -> ReflexLifetime {
    ReflexLifetime::UntilCancelled
}

const fn default_backend() -> Backend {
    Backend::Auto
}

const fn default_include_expired() -> bool {
    false
}

const fn default_history_limit() -> u32 {
    50
}

const MAX_REFLEX_HISTORY_LIMIT: u32 = 1000;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexRegisterParams {
    #[schemars(schema_with = "reflex_kind_schema")]
    pub kind: String,
    #[serde(default)]
    pub when: Option<ReflexWhenParam>,
    #[serde(default)]
    pub then: Option<ReflexThenParam>,
    #[serde(default)]
    pub debounce_ms: u32,
    #[serde(default)]
    pub target: Option<AimTarget>,
    #[serde(default)]
    pub axis: Option<ReflexAimAxis>,
    #[serde(default)]
    pub gain: Option<f32>,
    #[serde(default)]
    pub deadzone_px: Option<f32>,
    #[serde(default)]
    pub max_speed_px_per_tick: Option<f32>,
    #[serde(default)]
    pub ema_alpha: Option<f32>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub keys: Option<Vec<String>>,
    #[serde(default)]
    pub re_assert: bool,
    #[serde(default)]
    pub button: Option<ReflexButtonTarget>,
    #[serde(default)]
    pub steps: Option<Vec<ReflexComboStepParam>>,
    #[serde(default = "default_reflex_priority")]
    #[schemars(default = "default_reflex_priority", range(min = 0, max = 1000))]
    pub priority: u32,
    #[serde(default = "default_lifetime")]
    #[schemars(default = "default_lifetime")]
    pub lifetime: ReflexLifetime,
    #[serde(default = "default_backend")]
    #[schemars(default = "default_backend")]
    pub backend: Backend,
    #[serde(default)]
    pub exclusive: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ReflexWhenParam {
    Filter(EventFilter),
    WindowEvent(WindowEventWhen),
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowEventWhen {
    pub kind: String,
    #[serde(default, rename = "match")]
    pub match_clause: WindowEventMatch,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowEventMatch {
    #[serde(default)]
    pub window_title_regex: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ReflexThenParam {
    Core(ReflexThen),
    Steps { steps: Vec<ReflexThenStep> },
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexThenStep {
    pub action: String,
    #[serde(default = "empty_params")]
    pub params: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ReflexComboStepParam {
    Core(ComboStep),
    Tool(ReflexTimedThenStep),
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexTimedThenStep {
    #[serde(default)]
    pub at_ms: u32,
    pub action: String,
    #[serde(default = "empty_params")]
    pub params: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexRegisterResponse {
    pub reflex_id: String,
    pub state: ReflexStatus,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexCancelParams {
    pub reflex_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReflexCancelReason {
    Ok,
    NotFound,
    AlreadyExpired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexCancelResponse {
    pub cancelled: bool,
    pub reason: ReflexCancelReason,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexListParams {
    #[serde(default = "default_include_expired")]
    #[schemars(default = "default_include_expired")]
    pub include_expired: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexListResponse {
    pub reflexes: Vec<ReflexStatus>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexHistoryParams {
    #[serde(default)]
    pub reflex_id: Option<String>,
    #[serde(default = "default_history_limit")]
    #[schemars(default = "default_history_limit", range(min = 0, max = 1000))]
    pub limit: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflexHistoryResponse {
    pub events: Vec<StoredReflexAudit>,
}

#[must_use]
pub const fn reflex_register() -> M3ToolStub {
    M3ToolStub::new("reflex_register")
}

#[must_use]
pub const fn reflex_cancel() -> M3ToolStub {
    M3ToolStub::new("reflex_cancel")
}

#[must_use]
pub const fn reflex_list() -> M3ToolStub {
    M3ToolStub::new("reflex_list")
}

#[must_use]
pub const fn reflex_history() -> M3ToolStub {
    M3ToolStub::new("reflex_history")
}

pub fn required_permissions_register(
    params: &ReflexRegisterParams,
) -> Result<RequiredPermissions, ErrorData> {
    let mut permissions = required([Permission::WriteReflex]);
    let actions = actions_for_permissions(params)
        .map_err(|error| mcp_error(error.code(), error.to_string()))?;
    for action in &actions {
        add_action_permissions(action, &mut permissions);
    }
    Ok(permissions)
}

#[must_use]
pub fn required_permissions_cancel(_params: &ReflexCancelParams) -> RequiredPermissions {
    required([Permission::ReadReflex])
}

#[must_use]
pub fn required_permissions_list(_params: &ReflexListParams) -> RequiredPermissions {
    required([Permission::ReadReflex])
}

#[must_use]
pub fn required_permissions_history(_params: &ReflexHistoryParams) -> RequiredPermissions {
    required([Permission::ReadReflex])
}

pub fn register_reflex(
    runtime: &Arc<Mutex<ReflexRuntime>>,
    params: ReflexRegisterParams,
) -> Result<ReflexRegisterResponse, ErrorData> {
    let reflex = scheduled_reflex_from_params(params)
        .map_err(|error| mcp_error(error.code(), error.to_string()))?;
    let mut runtime = runtime.lock().map_err(|_err| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            "reflex runtime lock poisoned",
        )
    })?;
    let state = runtime
        .register(&reflex)
        .map_err(|error| mcp_error(error.code(), error.to_string()))?;
    drop(runtime);
    Ok(ReflexRegisterResponse {
        reflex_id: state.id.clone(),
        state,
    })
}

pub fn cancel_reflex(
    runtime: &Arc<Mutex<ReflexRuntime>>,
    params: &ReflexCancelParams,
) -> Result<ReflexCancelResponse, ErrorData> {
    let reflex_id = params.reflex_id.trim();
    if reflex_id.is_empty() {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            "reflex_cancel reflex_id must not be empty",
        ));
    }
    let mut runtime = runtime.lock().map_err(|_err| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            "reflex runtime lock poisoned",
        )
    })?;
    let outcome = runtime
        .cancel(reflex_id)
        .map_err(|error| mcp_error(error.code(), error.to_string()))?;
    drop(runtime);
    Ok(match outcome {
        ReflexCancelOutcome::Cancelled { .. } => ReflexCancelResponse {
            cancelled: true,
            reason: ReflexCancelReason::Ok,
        },
        ReflexCancelOutcome::NotFound => ReflexCancelResponse {
            cancelled: false,
            reason: ReflexCancelReason::NotFound,
        },
        ReflexCancelOutcome::AlreadyExpired { .. } => ReflexCancelResponse {
            cancelled: false,
            reason: ReflexCancelReason::AlreadyExpired,
        },
    })
}

pub fn list_reflexes(
    runtime: &Arc<Mutex<ReflexRuntime>>,
    params: &ReflexListParams,
) -> Result<ReflexListResponse, ErrorData> {
    let runtime = runtime.lock().map_err(|_err| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            "reflex runtime lock poisoned",
        )
    })?;
    let reflexes = runtime
        .list(params.include_expired)
        .map_err(|error| mcp_error(error.code(), error.to_string()))?;
    drop(runtime);
    Ok(ReflexListResponse { reflexes })
}

pub fn history_reflexes(
    runtime: &Arc<Mutex<ReflexRuntime>>,
    params: &ReflexHistoryParams,
) -> Result<ReflexHistoryResponse, ErrorData> {
    if params.limit > MAX_REFLEX_HISTORY_LIMIT {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("reflex_history limit must be <= {MAX_REFLEX_HISTORY_LIMIT}"),
        ));
    }
    let reflex_id = params.reflex_id.as_deref().map(str::trim);
    if reflex_id.is_some_and(str::is_empty) {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            "reflex_history reflex_id must not be empty",
        ));
    }

    let runtime = runtime.lock().map_err(|_err| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            "reflex runtime lock poisoned",
        )
    })?;
    let events = runtime
        .history(reflex_id, params.limit as usize)
        .map_err(|error| mcp_error(error.code(), error.to_string()))?;
    drop(runtime);
    Ok(ReflexHistoryResponse { events })
}

fn scheduled_reflex_from_params(
    params: ReflexRegisterParams,
) -> Result<ScheduledReflex, ReflexError> {
    let reflex_id = new_reflex_id();
    match params.kind.as_str() {
        "on_event" => {
            let when = params.when.ok_or_else(|| ReflexError::ParamsInvalid {
                detail: "on_event reflex requires when filter".to_owned(),
            })?;
            let when = when.into_event_filter()?;
            let actions =
                actions_from_then(required_then(params.then, "on_event")?, params.backend)?;
            let debounce = Duration::from_millis(u64::from(params.debounce_ms));
            let reflex = if debounce.is_zero() {
                ScheduledReflex::on_event(reflex_id, when, actions)
            } else {
                ScheduledReflex::on_event_with_debounce(reflex_id, when, actions, debounce)
            };
            Ok(reflex
                .with_priority(params.priority)
                .with_lifetime(params.lifetime)
                .with_exclusive(params.exclusive))
        }
        "aim_track" => {
            let mut aim_params = aim_track_params(&params)?;
            aim_params.backend = params.backend;
            Ok(ScheduledReflex::aim_track(reflex_id, aim_params)
                .with_priority(params.priority)
                .with_lifetime(params.lifetime)
                .with_exclusive(params.exclusive))
        }
        "hold_move" => {
            let hold_params = HoldMoveParams {
                keys: hold_move_keys(&params)?,
                backend: params.backend,
                re_assert: params.re_assert,
            };
            Ok(ScheduledReflex::hold_move(reflex_id, hold_params)
                .with_priority(params.priority)
                .with_lifetime(params.lifetime)
                .with_exclusive(params.exclusive))
        }
        "hold_button" => {
            let button = params.button.ok_or_else(|| ReflexError::ParamsInvalid {
                detail: "hold_button reflex requires button".to_owned(),
            })?;
            let hold_params = HoldButtonParams {
                button,
                backend: params.backend,
            };
            Ok(ScheduledReflex::hold_button(reflex_id, hold_params)
                .with_priority(params.priority)
                .with_lifetime(params.lifetime)
                .with_exclusive(params.exclusive))
        }
        "combo" => {
            let steps = combo_steps_from_params(params.steps, params.then)?;
            Ok(ScheduledReflex::every_tick(
                reflex_id,
                vec![Action::Combo {
                    steps,
                    backend: params.backend,
                }],
            )
            .with_priority(params.priority)
            .with_lifetime(ReflexLifetime::OneShot)
            .with_exclusive(params.exclusive))
        }
        other => Err(ReflexError::KindInvalid {
            detail: format!("unknown reflex kind: {other}"),
        }),
    }
}

fn actions_for_permissions(params: &ReflexRegisterParams) -> Result<Vec<Action>, ReflexError> {
    match params.kind.as_str() {
        "on_event" => actions_from_then(
            required_then(params.then.clone(), "on_event")?,
            params.backend,
        ),
        "aim_track" => Ok(vec![Action::MouseMoveRelative {
            dx: 0.0,
            dy: 0.0,
            backend: params.backend,
        }]),
        "hold_move" => hold_move_keys(params).map(|keys| {
            keys.into_iter()
                .map(|key| Action::KeyDown {
                    key,
                    backend: params.backend,
                })
                .collect()
        }),
        "hold_button" => {
            let button = params
                .button
                .clone()
                .ok_or_else(|| ReflexError::ParamsInvalid {
                    detail: "hold_button reflex requires button".to_owned(),
                })?;
            Ok(vec![button_down_action(&button, params.backend)])
        }
        "combo" => Ok(vec![Action::Combo {
            steps: combo_steps_from_params(params.steps.clone(), params.then.clone())?,
            backend: params.backend,
        }]),
        _other => Ok(Vec::new()),
    }
}

fn required_then(
    then: Option<ReflexThenParam>,
    kind: &'static str,
) -> Result<ReflexThenParam, ReflexError> {
    then.ok_or_else(|| ReflexError::ParamsInvalid {
        detail: format!("{kind} reflex requires then"),
    })
}

fn aim_track_params(params: &ReflexRegisterParams) -> Result<AimTrackParams, ReflexError> {
    let target = params
        .target
        .clone()
        .ok_or_else(|| ReflexError::ParamsInvalid {
            detail: "aim_track reflex requires target".to_owned(),
        })?;
    let mut aim_params = AimTrackParams::new(AimTrackTarget::from(target));
    if let Some(axis) = params.axis {
        aim_params.axis = axis;
    }
    if let Some(gain) = params.gain {
        aim_params.gain = gain;
    }
    if let Some(deadzone_px) = params.deadzone_px {
        aim_params.deadzone_px = deadzone_px;
    }
    if let Some(max_speed_px_per_tick) = params.max_speed_px_per_tick {
        aim_params.max_speed_px_per_tick = max_speed_px_per_tick;
    }
    if let Some(ema_alpha) = params.ema_alpha {
        aim_params.ema_alpha = ema_alpha;
    }
    Ok(aim_params)
}

fn hold_move_keys(params: &ReflexRegisterParams) -> Result<Vec<Key>, ReflexError> {
    let mut raw = Vec::new();
    if let Some(key) = &params.key {
        raw.push(key.clone());
    }
    if let Some(keys) = &params.keys {
        raw.extend(keys.clone());
    }
    if raw.is_empty() {
        return Err(ReflexError::ParamsInvalid {
            detail: "hold_move reflex requires key or keys".to_owned(),
        });
    }
    let mut seen = HashSet::new();
    raw.into_iter()
        .map(|raw_key| {
            let name = canonical_key_name(&raw_key)?;
            if !seen.insert(name.clone()) {
                return Err(ReflexError::ParamsInvalid {
                    detail: format!("hold_move duplicate key '{name}'"),
                });
            }
            Ok(named_key(&name))
        })
        .collect()
}

fn canonical_key_name(raw_key: &str) -> Result<String, ReflexError> {
    let lowered = raw_key.trim().to_ascii_lowercase();
    let key = match lowered.as_str() {
        "" => {
            return Err(ReflexError::ParamsInvalid {
                detail: "key names must be non-empty".to_owned(),
            });
        }
        "control" => "ctrl",
        "escape" => "esc",
        "return" => "enter",
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        "win" | "windows" | "meta" => "super",
        "pgup" => "pageup",
        "pgdn" => "pagedown",
        other => other,
    };

    if is_allowed_key_name(key) {
        Ok(key.to_owned())
    } else {
        Err(ReflexError::ParamsInvalid {
            detail: format!("unsupported key '{raw_key}'"),
        })
    }
}

fn is_allowed_key_name(key: &str) -> bool {
    if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() {
        return true;
    }
    if let Some(number) = key
        .strip_prefix('f')
        .and_then(|suffix| suffix.parse::<u8>().ok())
    {
        return (1..=24).contains(&number);
    }
    matches!(
        key,
        "alt"
            | "backspace"
            | "ctrl"
            | "delete"
            | "down"
            | "end"
            | "enter"
            | "esc"
            | "home"
            | "insert"
            | "left"
            | "pagedown"
            | "pageup"
            | "right"
            | "shift"
            | "space"
            | "super"
            | "tab"
            | "up"
    )
}

fn named_key(value: &str) -> Key {
    Key {
        code: KeyCode::Named {
            value: value.to_owned(),
        },
        use_scancode: false,
    }
}

const fn button_down_action(button: &ReflexButtonTarget, backend: Backend) -> Action {
    match *button {
        ReflexButtonTarget::Mouse { button } => Action::MouseButton {
            button,
            action: ButtonAction::Down,
            hold_ms: 0,
            backend,
        },
        ReflexButtonTarget::Pad { pad, button } => Action::PadButton {
            pad,
            button,
            action: ButtonAction::Down,
            hold_ms: 0,
        },
    }
}

fn combo_steps_from_params(
    steps: Option<Vec<ReflexComboStepParam>>,
    then: Option<ReflexThenParam>,
) -> Result<Vec<ComboStep>, ReflexError> {
    if let Some(steps) = steps {
        if steps.is_empty() {
            return Err(ReflexError::ParamsInvalid {
                detail: "combo steps must contain at least one step".to_owned(),
            });
        }
        return steps
            .into_iter()
            .enumerate()
            .map(|(index, step)| combo_step_from_param(index, step))
            .collect();
    }

    match then {
        Some(ReflexThenParam::Core(ReflexThen::Combo { steps, .. })) if !steps.is_empty() => {
            Ok(steps)
        }
        Some(ReflexThenParam::Core(ReflexThen::Combo { .. })) => Err(ReflexError::ParamsInvalid {
            detail: "combo steps must contain at least one step".to_owned(),
        }),
        Some(ReflexThenParam::Steps { steps }) => steps
            .into_iter()
            .enumerate()
            .map(|(index, step)| timed_demo_step_to_combo_step(index, 0, step))
            .collect(),
        Some(ReflexThenParam::Core(_)) | None => Err(ReflexError::ParamsInvalid {
            detail: "combo reflex requires steps or then.kind=combo".to_owned(),
        }),
    }
}

fn combo_step_from_param(
    index: usize,
    step: ReflexComboStepParam,
) -> Result<ComboStep, ReflexError> {
    match step {
        ReflexComboStepParam::Core(step) => Ok(step),
        ReflexComboStepParam::Tool(step) => {
            let at_ms = step.at_ms;
            let demo_step = ReflexThenStep {
                action: step.action,
                params: step.params,
            };
            timed_demo_step_to_combo_step(index, at_ms, demo_step)
        }
    }
}

fn timed_demo_step_to_combo_step(
    index: usize,
    at_ms: u32,
    step: ReflexThenStep,
) -> Result<ComboStep, ReflexError> {
    let action = action_from_demo_step(index, step)?;
    match action {
        Action::KeyPress { key, hold_ms, .. } => {
            let hold_ms = u16::try_from(hold_ms).map_err(|_err| ReflexError::ParamsInvalid {
                detail: format!("combo steps[{index}] hold_ms exceeds u16::MAX"),
            })?;
            Ok(ComboStep {
                at_ms,
                input: ComboInput::KeyPress { key, hold_ms },
            })
        }
        Action::MouseButton { button, action, .. } => Ok(ComboStep {
            at_ms,
            input: ComboInput::MouseButton { button, action },
        }),
        Action::MouseMoveRelative { dx, dy, .. } => Ok(ComboStep {
            at_ms,
            input: ComboInput::MouseMoveRel { dx, dy },
        }),
        other => Err(ReflexError::ParamsInvalid {
            detail: format!(
                "combo steps[{index}] action {other:?} cannot be used as one timed combo input"
            ),
        }),
    }
}

impl ReflexWhenParam {
    fn into_event_filter(self) -> Result<EventFilter, ReflexError> {
        match self {
            Self::Filter(filter) => Ok(filter),
            Self::WindowEvent(when) => when.into_event_filter(),
        }
    }
}

impl WindowEventWhen {
    fn into_event_filter(self) -> Result<EventFilter, ReflexError> {
        let kind = normalize_window_event_kind(&self.kind)?;
        let mut filters = vec![EventFilter::Kind { kind }];
        if let Some(pattern) = self.match_clause.window_title_regex {
            validate_regex(&pattern)?;
            filters.push(EventFilter::Data {
                path: "/window_title".to_owned(),
                predicate: DataPredicate::Regex { pattern },
            });
        }
        if filters.len() == 1 {
            Ok(filters.remove(0))
        } else {
            Ok(EventFilter::And { args: filters })
        }
    }
}

fn normalize_window_event_kind(raw: &str) -> Result<String, ReflexError> {
    let kind = raw.trim().replace('_', "-").to_ascii_lowercase();
    if kind.is_empty() {
        return Err(ReflexError::ParamsInvalid {
            detail: "window event kind must not be empty".to_owned(),
        });
    }
    Ok(kind)
}

fn validate_regex(pattern: &str) -> Result<(), ReflexError> {
    if pattern.trim().is_empty() {
        return Err(ReflexError::ParamsInvalid {
            detail: "window_title_regex must not be empty".to_owned(),
        });
    }
    regex::Regex::new(pattern).map_err(|error| ReflexError::ParamsInvalid {
        detail: format!("window_title_regex is invalid: {error}"),
    })?;
    Ok(())
}

fn actions_from_then(then: ReflexThenParam, backend: Backend) -> Result<Vec<Action>, ReflexError> {
    let mut actions = match then {
        ReflexThenParam::Core(ReflexThen::Action { action }) => vec![action],
        ReflexThenParam::Core(ReflexThen::Actions { actions }) => actions,
        ReflexThenParam::Core(ReflexThen::Combo {
            steps,
            backend: combo_backend,
        }) => vec![Action::Combo {
            steps,
            backend: combo_backend,
        }],
        ReflexThenParam::Steps { steps } => actions_from_demo_steps(steps)?,
    };
    for action in &mut actions {
        apply_backend_default(action, backend);
    }
    Ok(actions)
}

fn actions_from_demo_steps(steps: Vec<ReflexThenStep>) -> Result<Vec<Action>, ReflexError> {
    if steps.is_empty() {
        return Err(ReflexError::ParamsInvalid {
            detail: "then.steps must contain at least one action".to_owned(),
        });
    }
    steps
        .into_iter()
        .enumerate()
        .map(|(index, step)| action_from_demo_step(index, step))
        .collect()
}

fn action_from_demo_step(index: usize, step: ReflexThenStep) -> Result<Action, ReflexError> {
    match step.action.trim() {
        "act_type" => {
            let params = serde_json::from_value::<ActTypeParams>(step.params).map_err(|error| {
                ReflexError::ParamsInvalid {
                    detail: format!("then.steps[{index}].act_type params invalid: {error}"),
                }
            })?;
            action_from_type_params(&params).map_err(|error| ReflexError::ParamsInvalid {
                detail: format!("then.steps[{index}].act_type params invalid: {error}"),
            })
        }
        "act_press" => {
            let params =
                serde_json::from_value::<ActPressParams>(step.params).map_err(|error| {
                    ReflexError::ParamsInvalid {
                        detail: format!("then.steps[{index}].act_press params invalid: {error}"),
                    }
                })?;
            action_from_press_params(&params).map_err(|error| ReflexError::ParamsInvalid {
                detail: format!("then.steps[{index}].act_press params invalid: {error}"),
            })
        }
        other => Err(ReflexError::ParamsInvalid {
            detail: format!(
                "then.steps[{index}].action {other:?} is unsupported; supported actions: act_type, act_press"
            ),
        }),
    }
}

fn empty_params() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

fn apply_backend_default(action: &mut Action, fallback: Backend) {
    if fallback == Backend::Auto {
        return;
    }
    match action {
        Action::KeyPress { backend, .. }
        | Action::KeyDown { backend, .. }
        | Action::KeyUp { backend, .. }
        | Action::KeyChord { backend, .. }
        | Action::TypeText { backend, .. }
        | Action::MouseMove { backend, .. }
        | Action::MouseMoveRelative { backend, .. }
        | Action::MouseButton { backend, .. }
        | Action::MouseDrag { backend, .. }
        | Action::MouseScroll { backend, .. }
        | Action::AimAt { backend, .. }
        | Action::Combo { backend, .. }
            if *backend == Backend::Auto =>
        {
            *backend = fallback;
        }
        Action::KeyPress { .. }
        | Action::KeyDown { .. }
        | Action::KeyUp { .. }
        | Action::KeyChord { .. }
        | Action::TypeText { .. }
        | Action::MouseMove { .. }
        | Action::MouseMoveRelative { .. }
        | Action::MouseButton { .. }
        | Action::MouseDrag { .. }
        | Action::MouseScroll { .. }
        | Action::AimAt { .. }
        | Action::Combo { .. }
        | Action::PadButton { .. }
        | Action::PadStick { .. }
        | Action::PadTrigger { .. }
        | Action::PadReport { .. }
        | Action::ReleaseAll => {}
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::float_cmp,
    reason = "unit tests intentionally assert exact schema-mapping values and failure paths"
)]
mod tests {
    use serde_json::json;
    use synapse_core::{
        Action, Backend, ComboInput, DataPredicate, EventFilter, KeyCode, KeystrokeDynamics,
        KeystrokeNaturalParams, ReflexAimAxis, ReflexLifetime,
    };
    use synapse_reflex::{AimTrackTarget, ScheduledReflexDriver, SchedulerTrigger};

    use super::{
        ReflexRegisterParams, required_permissions_register, scheduled_reflex_from_params,
    };
    use crate::m3::permissions::Permission;

    #[test]
    fn demo_gate_shape_maps_to_event_filter_and_actions() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "on_event",
            "when": {
                "kind": "element-appeared",
                "match": { "window_title_regex": "^Save As$" }
            },
            "then": {
                "steps": [
                    {
                        "action": "act_type",
                        "params": {
                            "text": "m3-demo.txt",
                            "dynamics": "linear",
                            "linear_ms_per_char": 20
                        }
                    },
                    {
                        "action": "act_press",
                        "params": { "keys": ["enter"] }
                    }
                ]
            },
            "lifetime": { "kind": "one_shot" }
        }))
        .expect("demo shape should deserialize");

        let reflex =
            scheduled_reflex_from_params(params).expect("demo shape should build a reflex");

        let SchedulerTrigger::OnEvent(EventFilter::And { args }) = reflex.trigger else {
            panic!("demo when should map to a compound on_event filter");
        };
        assert!(args.contains(&EventFilter::Kind {
            kind: "element-appeared".to_owned()
        }));
        assert!(args.contains(&EventFilter::Data {
            path: "/window_title".to_owned(),
            predicate: DataPredicate::Regex {
                pattern: "^Save As$".to_owned()
            }
        }));
        assert_eq!(reflex.lifetime, ReflexLifetime::OneShot);
        assert_eq!(reflex.then.len(), 2);
        match &reflex.then[0] {
            Action::TypeText { text, dynamics, .. } => {
                assert_eq!(text, "m3-demo.txt");
                assert_eq!(dynamics, &KeystrokeDynamics::Linear { ms_per_char: 20 });
            }
            other => panic!("first demo step should map to TypeText, got {other:?}"),
        }
        match &reflex.then[1] {
            Action::KeyPress { key, hold_ms, .. } => {
                assert_eq!(*hold_ms, 33);
                assert_eq!(
                    &key.code,
                    &KeyCode::Named {
                        value: "enter".to_owned()
                    }
                );
            }
            other => panic!("second demo step should map to KeyPress, got {other:?}"),
        }
    }

    #[test]
    fn demo_step_act_type_omitted_dynamics_resolves_to_natural_fast() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "on_event",
            "when": { "op": "kind", "kind": "support-default-resolution" },
            "then": {
                "steps": [
                    {
                        "action": "act_type",
                        "params": { "text": "abc" }
                    }
                ]
            }
        }))
        .expect("default dynamics reflex shape should deserialize");

        let reflex =
            scheduled_reflex_from_params(params).expect("default dynamics reflex should build");

        assert_eq!(reflex.then.len(), 1);
        match &reflex.then[0] {
            Action::TypeText {
                text,
                dynamics,
                backend,
            } => {
                assert_eq!(text, "abc");
                assert_eq!(
                    dynamics,
                    &KeystrokeDynamics::Natural {
                        params: KeystrokeNaturalParams::FAST
                    }
                );
                assert_eq!(*backend, Backend::Auto);
            }
            other => panic!("default act_type step should map to TypeText, got {other:?}"),
        }
    }

    #[test]
    fn on_event_debounce_ms_maps_to_scheduler_debounce() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "on_event",
            "when": { "op": "kind", "kind": "value-changed" },
            "then": { "kind": "action", "action": { "kind": "release_all" } },
            "debounce_ms": 250
        }))
        .expect("debounced reflex shape should deserialize");

        let reflex =
            scheduled_reflex_from_params(params).expect("debounced on_event should build a reflex");

        assert_eq!(reflex.debounce, std::time::Duration::from_millis(250));
    }

    #[test]
    fn aim_track_shape_maps_to_stateful_driver() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "aim_track",
            "target": { "kind": "screen", "point": { "x": 100, "y": 200 } },
            "axis": "x_only",
            "gain": 0.5,
            "deadzone_px": 2.0,
            "max_speed_px_per_tick": 40.0,
            "backend": "software"
        }))
        .expect("aim_track reflex shape should deserialize");

        let reflex = scheduled_reflex_from_params(params).expect("aim_track should build a reflex");

        let ScheduledReflexDriver::AimTrack(params) = reflex.driver else {
            panic!("aim_track should map to stateful driver");
        };
        assert_eq!(params.target, AimTrackTarget::Point(point(100, 200)));
        assert_eq!(params.axis, ReflexAimAxis::XOnly);
        assert_eq!(params.gain, 0.5);
        assert_eq!(params.deadzone_px, 2.0);
        assert_eq!(params.max_speed_px_per_tick, 40.0);
        assert_eq!(params.backend, Backend::Software);
    }

    #[test]
    fn hold_move_shape_maps_key_string_and_duration() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "hold_move",
            "key": "w",
            "lifetime": { "kind": "duration", "ms": 1500 },
            "backend": "software"
        }))
        .expect("hold_move reflex shape should deserialize");

        let reflex = scheduled_reflex_from_params(params).expect("hold_move should build a reflex");

        let ScheduledReflexDriver::HoldMove(params) = reflex.driver else {
            panic!("hold_move should map to stateful driver");
        };
        assert_eq!(reflex.lifetime, ReflexLifetime::Duration { ms: 1500 });
        assert_eq!(params.keys.len(), 1);
        assert_eq!(
            params.keys[0].code,
            KeyCode::Named {
                value: "w".to_owned()
            }
        );
        assert_eq!(params.backend, Backend::Software);
    }

    #[test]
    fn combo_timed_act_press_steps_map_to_one_shot_combo_action() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "combo",
            "steps": [
                { "at_ms": 0, "action": "act_press", "params": { "keys": ["e"] } },
                { "at_ms": 200, "action": "act_press", "params": { "keys": ["space"] } }
            ],
            "backend": "software"
        }))
        .expect("combo reflex shape should deserialize");

        let reflex = scheduled_reflex_from_params(params).expect("combo should build a reflex");

        assert!(matches!(reflex.driver, ScheduledReflexDriver::Actions));
        assert_eq!(reflex.lifetime, ReflexLifetime::OneShot);
        let [Action::Combo { steps, backend }] = reflex.then.as_slice() else {
            panic!("combo should map to one combo action");
        };
        assert_eq!(*backend, Backend::Software);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].at_ms, 0);
        assert_eq!(steps[1].at_ms, 200);
        assert!(matches!(
            steps[0].input,
            ComboInput::KeyPress { ref key, hold_ms: 33 }
                if key.code == KeyCode::Named { value: "e".to_owned() }
        ));
        assert!(matches!(
            steps[1].input,
            ComboInput::KeyPress { ref key, hold_ms: 33 }
                if key.code == KeyCode::Named { value: "space".to_owned() }
        ));
    }

    #[test]
    fn register_permissions_bubble_then_step_backend_requirements() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "on_event",
            "when": { "op": "kind", "kind": "support-permissions" },
            "then": {
                "steps": [
                    {
                        "action": "act_type",
                        "params": { "text": "abc" }
                    }
                ]
            },
            "backend": "hardware"
        }))
        .expect("permission reflex shape should deserialize");

        let permissions =
            required_permissions_register(&params).expect("permission calculation should pass");
        assert!(permissions.contains(&Permission::WriteReflex));
        assert!(permissions.contains(&Permission::InputKeyboard));
        assert!(permissions.contains(&Permission::InputHardwareHid));
        assert!(!permissions.contains(&Permission::InputMouse));
        assert!(!permissions.contains(&Permission::InputPad));
    }

    #[test]
    fn demo_gate_shape_rejects_invalid_regex() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "on_event",
            "when": {
                "kind": "element-appeared",
                "match": { "window_title_regex": "[" }
            },
            "then": { "steps": [{ "action": "act_press", "params": { "keys": ["enter"] } }] }
        }))
        .expect("invalid regex still deserializes before validation");

        let error = scheduled_reflex_from_params(params)
            .expect_err("invalid window_title_regex should fail closed");
        assert!(
            error.to_string().contains("window_title_regex is invalid"),
            "{error}"
        );
    }

    #[test]
    fn demo_gate_shape_rejects_unknown_action() {
        let params: ReflexRegisterParams = serde_json::from_value(json!({
            "kind": "on_event",
            "when": { "kind": "element-appeared" },
            "then": { "steps": [{ "action": "act_launch", "params": {} }] }
        }))
        .expect("unknown action still deserializes before validation");

        let error = scheduled_reflex_from_params(params)
            .expect_err("unsupported reflex step should fail closed");
        assert!(error.to_string().contains("unsupported"), "{error}");
    }

    const fn point(x: i32, y: i32) -> synapse_core::Point {
        synapse_core::Point { x, y }
    }
}
