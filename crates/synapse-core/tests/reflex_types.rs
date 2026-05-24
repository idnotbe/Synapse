#![allow(clippy::missing_const_for_fn)]

use std::fmt::Debug;

use chrono::{DateTime, Duration, Utc};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRng, TestRunner},
};
use serde::{Serialize, de::DeserializeOwned};
use synapse_core::{
    Action, AimCurve, AimNaturalParams, AimTarget, Backend, ButtonAction, ComboInput, ComboStep,
    DataPredicate, EventFilter, EventSource, Key, KeyCode, MouseButton, PadButton, Point,
    ReflexAimAxis, ReflexButtonTarget, ReflexKind, ReflexLifetime, ReflexRegistration, ReflexState,
    ReflexStatus, ReflexThen,
};

#[test]
fn reflex_type_edge_round_trips_with_readback() -> Result<(), Box<dyn std::error::Error>> {
    round_trip("ReflexRegistration", "empty", empty_registration())?;
    round_trip(
        "ReflexRegistration",
        "required_only",
        required_registration(),
    )?;
    round_trip("ReflexRegistration", "fully_populated", full_registration())?;

    round_trip("ReflexKind", "empty", empty_kind())?;
    round_trip("ReflexKind", "required_only", hold_move_kind())?;
    round_trip("ReflexKind", "fully_populated", full_on_event_kind())?;

    round_trip("ReflexLifetime", "empty", ReflexLifetime::UntilCancelled)?;
    round_trip("ReflexLifetime", "required_only", ReflexLifetime::OneShot)?;
    round_trip("ReflexLifetime", "fully_populated", full_lifetime())?;

    round_trip("ReflexState", "empty", ReflexState::Active)?;
    round_trip("ReflexState", "required_only", ReflexState::Paused)?;
    round_trip("ReflexState", "fully_populated", ReflexState::Starved)?;

    round_trip("ReflexStatus", "empty", empty_status())?;
    round_trip("ReflexStatus", "required_only", required_status())?;
    round_trip("ReflexStatus", "fully_populated", full_status())?;

    round_trip("ReflexThen", "empty", empty_then())?;
    round_trip("ReflexThen", "required_only", action_then("space"))?;
    round_trip("ReflexThen", "fully_populated", full_then())?;

    round_trip(
        "ReflexButtonTarget",
        "empty",
        ReflexButtonTarget::Mouse {
            button: MouseButton::Left,
        },
    )?;
    round_trip(
        "ReflexButtonTarget",
        "required_only",
        ReflexButtonTarget::Mouse {
            button: MouseButton::Right,
        },
    )?;
    round_trip(
        "ReflexButtonTarget",
        "fully_populated",
        ReflexButtonTarget::Pad {
            pad: 1,
            button: PadButton::Rb,
        },
    )?;

    round_trip("ReflexAimAxis", "empty", ReflexAimAxis::Xy)?;
    round_trip("ReflexAimAxis", "required_only", ReflexAimAxis::XOnly)?;
    round_trip("ReflexAimAxis", "fully_populated", ReflexAimAxis::YOnly)?;

    Ok(())
}

#[test]
fn reflex_type_json_snapshots() -> Result<(), Box<dyn std::error::Error>> {
    insta::assert_json_snapshot!(
        "reflex_registration_round_trip",
        round_trip("ReflexRegistration", "snapshot", full_registration())?
    );
    insta::assert_json_snapshot!(
        "reflex_kind_round_trip",
        round_trip("ReflexKind", "snapshot", full_on_event_kind())?
    );
    insta::assert_json_snapshot!(
        "reflex_lifetime_round_trip",
        round_trip("ReflexLifetime", "snapshot", full_lifetime())?
    );
    insta::assert_json_snapshot!(
        "reflex_state_round_trip",
        round_trip("ReflexState", "snapshot", ReflexState::Starved)?
    );
    insta::assert_json_snapshot!(
        "reflex_status_round_trip",
        round_trip("ReflexStatus", "snapshot", full_status())?
    );
    insta::assert_json_snapshot!(
        "reflex_then_round_trip",
        round_trip("ReflexThen", "snapshot", full_then())?
    );
    insta::assert_json_snapshot!(
        "reflex_button_target_round_trip",
        round_trip(
            "ReflexButtonTarget",
            "snapshot",
            ReflexButtonTarget::Pad {
                pad: 1,
                button: PadButton::Rb,
            },
        )?
    );
    insta::assert_json_snapshot!(
        "reflex_aim_axis_round_trip",
        round_trip("ReflexAimAxis", "snapshot", ReflexAimAxis::YOnly)?
    );

    Ok(())
}

#[test]
fn reflex_types_proptest_json_round_trip_is_deterministic() -> Result<(), Box<dyn std::error::Error>>
{
    assert_strategy_round_trips("ReflexRegistration", reflex_registration_strategy())?;
    assert_strategy_round_trips("ReflexKind", reflex_kind_strategy())?;
    assert_strategy_round_trips("ReflexLifetime", reflex_lifetime_strategy())?;
    assert_strategy_round_trips("ReflexState", reflex_state_strategy())?;
    assert_strategy_round_trips("ReflexStatus", reflex_status_strategy())?;
    assert_strategy_round_trips("ReflexThen", reflex_then_strategy())?;
    assert_strategy_round_trips("ReflexButtonTarget", reflex_button_target_strategy())?;
    assert_strategy_round_trips("ReflexAimAxis", reflex_aim_axis_strategy())?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn round_trip<T>(type_name: &str, edge: &str, value: T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Clone + Debug + PartialEq + Serialize + DeserializeOwned + 'static,
{
    let before = serde_json::to_value(value.clone())?;
    println!("readback=serde_round_trip type={type_name} edge={edge} before={before}");
    let parsed = serde_json::from_value::<T>(before)?;
    let after = serde_json::to_value(&parsed)?;
    println!(
        "readback=serde_round_trip type={type_name} edge={edge} after={after} result_value={after}"
    );
    assert_eq!(parsed, value);
    Ok(parsed)
}

fn assert_strategy_round_trips<T, S>(
    type_name: &str,
    strategy: S,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: Clone + Debug + PartialEq + Serialize + DeserializeOwned + 'static,
    S: Strategy<Value = T>,
{
    let config = Config {
        cases: 1_000,
        failure_persistence: None,
        ..Config::default()
    };
    let algorithm = config.rng_algorithm;
    let mut runner = TestRunner::new_with_rng(config, TestRng::deterministic_rng(algorithm));

    println!("readback=serde_round_trip_proptest type={type_name} before=cases:1000");
    runner.run(&strategy, |value| {
        let json = serde_json::to_value(value.clone())?;
        let parsed = serde_json::from_value::<T>(json)?;
        prop_assert_eq!(parsed, value);
        Ok(())
    })?;
    println!(
        "readback=serde_round_trip_proptest type={type_name} after=cases:1000 result_value=all_round_tripped"
    );
    Ok(())
}

fn empty_registration() -> ReflexRegistration {
    ReflexRegistration {
        id: String::new(),
        kind: empty_kind(),
        priority: 0,
        lifetime: ReflexLifetime::UntilCancelled,
        exclusive: false,
    }
}

fn required_registration() -> ReflexRegistration {
    ReflexRegistration {
        id: "reflex-required".to_owned(),
        kind: hold_move_kind(),
        priority: 100,
        lifetime: ReflexLifetime::UntilCancelled,
        exclusive: false,
    }
}

fn full_registration() -> ReflexRegistration {
    ReflexRegistration {
        id: "reflex-fully-populated".to_owned(),
        kind: full_on_event_kind(),
        priority: 900,
        lifetime: full_lifetime(),
        exclusive: true,
    }
}

fn empty_kind() -> ReflexKind {
    ReflexKind::Combo {
        steps: Vec::new(),
        backend: Backend::Auto,
    }
}

fn hold_move_kind() -> ReflexKind {
    ReflexKind::HoldMove {
        keys: vec![key_named("w")],
        backend: Backend::Software,
        re_assert: false,
    }
}

fn full_on_event_kind() -> ReflexKind {
    ReflexKind::OnEvent {
        when: full_filter(),
        then: full_then(),
        debounce_ms: 250,
    }
}

fn full_lifetime() -> ReflexLifetime {
    ReflexLifetime::UntilEvent {
        filter: EventFilter::Kind {
            kind: "entity_disappeared".to_owned(),
        },
    }
}

fn empty_status() -> ReflexStatus {
    ReflexStatus {
        id: String::new(),
        kind_summary: String::new(),
        state: ReflexState::Active,
        registered_at: fixed_time(0),
        last_fired_at: None,
        fire_count: 0,
        priority: 0,
        lifetime: ReflexLifetime::UntilCancelled,
        exclusive: false,
        last_error_code: None,
    }
}

fn required_status() -> ReflexStatus {
    ReflexStatus {
        id: "reflex-required".to_owned(),
        kind_summary: "hold_move".to_owned(),
        state: ReflexState::Active,
        registered_at: fixed_time(1),
        last_fired_at: None,
        fire_count: 0,
        priority: 100,
        lifetime: ReflexLifetime::UntilCancelled,
        exclusive: false,
        last_error_code: None,
    }
}

fn full_status() -> ReflexStatus {
    ReflexStatus {
        id: "reflex-starved".to_owned(),
        kind_summary: "aim_track(track:42)".to_owned(),
        state: ReflexState::Starved,
        registered_at: fixed_time(10),
        last_fired_at: Some(fixed_time(12)),
        fire_count: 64,
        priority: 250,
        lifetime: ReflexLifetime::Duration { ms: 5_000 },
        exclusive: true,
        last_error_code: Some("REFLEX_STARVED".to_owned()),
    }
}

fn empty_then() -> ReflexThen {
    ReflexThen::Actions {
        actions: Vec::new(),
    }
}

fn action_then(key: &str) -> ReflexThen {
    ReflexThen::Action {
        action: key_press_action(key),
    }
}

fn full_then() -> ReflexThen {
    ReflexThen::Actions {
        actions: vec![
            key_press_action("e"),
            Action::MouseButton {
                button: MouseButton::Left,
                action: ButtonAction::Press,
                hold_ms: 16,
                backend: Backend::Software,
            },
        ],
    }
}

fn full_filter() -> EventFilter {
    EventFilter::And {
        args: vec![
            EventFilter::Kind {
                kind: "hud_value_changed".to_owned(),
            },
            EventFilter::Source {
                source: EventSource::PerceptionHud,
            },
            EventFilter::Data {
                path: "/field".to_owned(),
                predicate: DataPredicate::Eq {
                    value: serde_json::json!("hp"),
                },
            },
            EventFilter::Data {
                path: "/new".to_owned(),
                predicate: DataPredicate::Lt {
                    value: serde_json::json!(20),
                },
            },
        ],
    }
}

fn key_press_action(key: &str) -> Action {
    Action::KeyPress {
        key: key_named(key),
        hold_ms: 30,
        backend: Backend::Software,
    }
}

fn key_named(value: &str) -> Key {
    Key {
        code: KeyCode::Named {
            value: value.to_owned(),
        },
        use_scancode: false,
    }
}

fn fixed_time(offset_seconds: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from(std::time::SystemTime::UNIX_EPOCH) + Duration::seconds(offset_seconds)
}

fn small_string() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,8}".prop_map(|value| value)
}

fn backend_strategy() -> impl Strategy<Value = Backend> {
    prop_oneof![
        Just(Backend::Software),
        Just(Backend::Vigem),
        Just(Backend::Hardware),
        Just(Backend::Auto),
    ]
}

fn key_strategy() -> impl Strategy<Value = Key> {
    prop_oneof![
        small_string().prop_map(|value| Key {
            code: KeyCode::Named { value },
            use_scancode: false,
        }),
        (0_u8..128, any::<bool>()).prop_map(|(value, use_scancode)| Key {
            code: KeyCode::HidCode { value },
            use_scancode,
        }),
    ]
}

fn point_strategy() -> impl Strategy<Value = Point> {
    (-10_000_i32..10_000, -10_000_i32..10_000).prop_map(|(x, y)| Point { x, y })
}

fn aim_target_strategy() -> impl Strategy<Value = AimTarget> {
    prop_oneof![
        point_strategy().prop_map(|point| AimTarget::Screen { point }),
        (1_u64..10_000).prop_map(|track_id| AimTarget::Track { track_id }),
    ]
}

fn aim_curve_strategy() -> impl Strategy<Value = AimCurve> {
    prop_oneof![
        Just(AimCurve::Instant),
        Just(AimCurve::Linear),
        (0.0_f32..1.0, 0.0_f32..1.0, 0.0_f32..1.0, 0.0_f32..1.0).prop_map(
            |(p1x, p1y, p2x, p2y)| AimCurve::Bezier {
                p1: (p1x, p1y),
                p2: (p2x, p2y),
            },
        ),
        Just(AimCurve::Natural {
            params: AimNaturalParams::FAST,
        }),
    ]
}

fn mouse_button_strategy() -> impl Strategy<Value = MouseButton> {
    prop_oneof![
        Just(MouseButton::Left),
        Just(MouseButton::Right),
        Just(MouseButton::Middle),
        Just(MouseButton::X1),
        Just(MouseButton::X2),
    ]
}

fn button_action_strategy() -> impl Strategy<Value = ButtonAction> {
    prop_oneof![
        Just(ButtonAction::Press),
        Just(ButtonAction::Down),
        Just(ButtonAction::Up),
    ]
}

fn pad_button_strategy() -> impl Strategy<Value = PadButton> {
    prop_oneof![
        Just(PadButton::A),
        Just(PadButton::B),
        Just(PadButton::X),
        Just(PadButton::Y),
        Just(PadButton::Lb),
        Just(PadButton::Rb),
    ]
}

fn reflex_aim_axis_strategy() -> impl Strategy<Value = ReflexAimAxis> {
    prop_oneof![
        Just(ReflexAimAxis::Xy),
        Just(ReflexAimAxis::XOnly),
        Just(ReflexAimAxis::YOnly),
    ]
}

fn reflex_button_target_strategy() -> impl Strategy<Value = ReflexButtonTarget> {
    prop_oneof![
        mouse_button_strategy().prop_map(|button| ReflexButtonTarget::Mouse { button }),
        (0_u8..4, pad_button_strategy())
            .prop_map(|(pad, button)| ReflexButtonTarget::Pad { pad, button }),
    ]
}

fn combo_input_strategy() -> impl Strategy<Value = ComboInput> {
    prop_oneof![
        key_strategy().prop_map(|key| ComboInput::KeyDown { key }),
        key_strategy().prop_map(|key| ComboInput::KeyUp { key }),
        (key_strategy(), 0_u16..250)
            .prop_map(|(key, hold_ms)| ComboInput::KeyPress { key, hold_ms }),
        (mouse_button_strategy(), button_action_strategy())
            .prop_map(|(button, action)| { ComboInput::MouseButton { button, action } }),
        (-100.0_f32..100.0, -100.0_f32..100.0)
            .prop_map(|(dx, dy)| ComboInput::MouseMoveRel { dx, dy }),
    ]
}

fn combo_step_strategy() -> impl Strategy<Value = ComboStep> {
    (0_u32..5_000, combo_input_strategy()).prop_map(|(at_ms, input)| ComboStep { at_ms, input })
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        (key_strategy(), 0_u32..250, backend_strategy()).prop_map(|(key, hold_ms, backend)| {
            Action::KeyPress {
                key,
                hold_ms,
                backend,
            }
        }),
        (key_strategy(), backend_strategy())
            .prop_map(|(key, backend)| Action::KeyDown { key, backend }),
        (
            mouse_button_strategy(),
            button_action_strategy(),
            0_u32..250,
            backend_strategy()
        )
            .prop_map(|(button, action, hold_ms, backend)| Action::MouseButton {
                button,
                action,
                hold_ms,
                backend,
            }),
        (-500.0_f32..500.0, -500.0_f32..500.0, backend_strategy())
            .prop_map(|(dx, dy, backend)| Action::MouseMoveRelative { dx, dy, backend },),
    ]
}

fn event_filter_strategy() -> impl Strategy<Value = EventFilter> {
    prop_oneof![
        Just(EventFilter::All),
        Just(EventFilter::None),
        small_string().prop_map(|kind| EventFilter::Kind { kind }),
        Just(EventFilter::Source {
            source: EventSource::Reflex,
        }),
        small_string().prop_map(|kind| EventFilter::Not {
            arg: Box::new(EventFilter::Kind { kind }),
        }),
        (small_string(), small_string()).prop_map(|(path, value)| EventFilter::Data {
            path: format!("/{path}"),
            predicate: DataPredicate::Eq {
                value: serde_json::json!(value),
            },
        }),
    ]
}

fn reflex_then_strategy() -> impl Strategy<Value = ReflexThen> {
    prop_oneof![
        action_strategy().prop_map(|action| ReflexThen::Action { action }),
        prop::collection::vec(action_strategy(), 0..4)
            .prop_map(|actions| ReflexThen::Actions { actions }),
        (
            prop::collection::vec(combo_step_strategy(), 0..4),
            backend_strategy(),
        )
            .prop_map(|(steps, backend)| ReflexThen::Combo { steps, backend }),
    ]
}

fn reflex_lifetime_strategy() -> impl Strategy<Value = ReflexLifetime> {
    prop_oneof![
        Just(ReflexLifetime::UntilCancelled),
        Just(ReflexLifetime::OneShot),
        (0_u32..60_000).prop_map(|ms| ReflexLifetime::Duration { ms }),
        event_filter_strategy().prop_map(|filter| ReflexLifetime::UntilEvent { filter }),
        (0_u32..3_600_000).prop_map(|ms| ReflexLifetime::UntilDeadline { ms }),
    ]
}

fn reflex_kind_strategy() -> impl Strategy<Value = ReflexKind> {
    prop_oneof![
        (
            aim_target_strategy(),
            reflex_aim_axis_strategy(),
            0.0_f32..2.0,
            0.0_f32..50.0,
            1.0_f32..5_000.0,
            aim_curve_strategy(),
            backend_strategy(),
        )
            .prop_map(
                |(
                    target,
                    axis,
                    gain,
                    deadzone_px,
                    max_speed_px_per_ms,
                    curve_per_step,
                    backend,
                )| ReflexKind::AimTrack {
                    target,
                    axis,
                    gain,
                    deadzone_px,
                    max_speed_px_per_ms,
                    curve_per_step,
                    backend,
                },
            ),
        (
            prop::collection::vec(key_strategy(), 0..4),
            backend_strategy(),
            any::<bool>(),
        )
            .prop_map(|(keys, backend, re_assert)| ReflexKind::HoldMove {
                keys,
                backend,
                re_assert,
            }),
        (reflex_button_target_strategy(), backend_strategy())
            .prop_map(|(button, backend)| { ReflexKind::HoldButton { button, backend } }),
        (
            prop::collection::vec(combo_step_strategy(), 0..4),
            backend_strategy(),
        )
            .prop_map(|(steps, backend)| ReflexKind::Combo { steps, backend }),
        (
            event_filter_strategy(),
            reflex_then_strategy(),
            0_u32..10_000
        )
            .prop_map(|(when, then, debounce_ms)| ReflexKind::OnEvent {
                when,
                then,
                debounce_ms,
            },),
    ]
}

fn reflex_registration_strategy() -> impl Strategy<Value = ReflexRegistration> {
    (
        small_string(),
        reflex_kind_strategy(),
        0_u32..1_001,
        reflex_lifetime_strategy(),
        any::<bool>(),
    )
        .prop_map(
            |(id, kind, priority, lifetime, exclusive)| ReflexRegistration {
                id,
                kind,
                priority,
                lifetime,
                exclusive,
            },
        )
}

fn reflex_state_strategy() -> impl Strategy<Value = ReflexState> {
    prop_oneof![
        Just(ReflexState::Active),
        Just(ReflexState::Paused),
        Just(ReflexState::Cancelled),
        Just(ReflexState::Expired),
        Just(ReflexState::Disabled),
        Just(ReflexState::Starved),
    ]
}

fn instant_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    (0_i64..86_400).prop_map(fixed_time)
}

fn reflex_status_strategy() -> impl Strategy<Value = ReflexStatus> {
    (
        small_string(),
        small_string(),
        reflex_state_strategy(),
        instant_strategy(),
        prop::option::of(instant_strategy()),
        0_u64..10_000,
        0_u32..1_001,
        reflex_lifetime_strategy(),
        any::<bool>(),
        prop::option::of(small_string()),
    )
        .prop_map(
            |(
                id,
                kind_summary,
                state,
                registered_at,
                last_fired_at,
                fire_count,
                priority,
                lifetime,
                exclusive,
                last_error_code,
            )| ReflexStatus {
                id,
                kind_summary,
                state,
                registered_at,
                last_fired_at,
                fire_count,
                priority,
                lifetime,
                exclusive,
                last_error_code,
            },
        )
}
