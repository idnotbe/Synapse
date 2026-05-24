#![allow(clippy::missing_const_for_fn)]

use std::{collections::BTreeMap, fmt::Debug};

use chrono::{DateTime, Duration, Utc};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRng, TestRunner},
};
use serde::{Serialize, de::DeserializeOwned};
use synapse_core::{
    AccessibleNode, Action, AudioContext, AudioEvent, Backend, ButtonAction, ClipboardSummary,
    DetectedEntity, DirectionEstimate, ElementId, EventSource, EventSummary, FocusedElement,
    ForegroundContext, FsEvent, FsEventKind, HudReading, HudReadings, HudValue, Key, KeyCode,
    MouseButton, ObservationDiagnostics, PerceptionMode, Rect, ReflexState, SCHEMA_VERSION,
    SensorStatus, StoredEvent, StoredObservation, StoredProfileHistoryEntry, StoredRedaction,
    StoredReflexAudit, StoredReflexStep, StoredSession, UiaPattern, element_id, entity_id,
};

#[test]
fn stored_type_edge_round_trips_with_fsv() -> Result<(), Box<dyn std::error::Error>> {
    round_trip("StoredEvent", "empty", empty_event())?;
    round_trip("StoredEvent", "required_only", required_event())?;
    round_trip("StoredEvent", "fully_populated", full_event())?;

    round_trip("StoredObservation", "empty", empty_observation())?;
    round_trip("StoredObservation", "required_only", required_observation())?;
    round_trip("StoredObservation", "fully_populated", full_observation())?;

    round_trip("StoredReflexAudit", "empty", empty_reflex_audit())?;
    round_trip(
        "StoredReflexAudit",
        "required_only",
        required_reflex_audit(),
    )?;
    round_trip("StoredReflexAudit", "fully_populated", full_reflex_audit())?;

    round_trip("StoredSession", "empty", empty_session())?;
    round_trip("StoredSession", "required_only", required_session())?;
    round_trip("StoredSession", "fully_populated", full_session())?;

    round_trip("StoredRedaction", "empty", empty_redaction())?;
    round_trip("StoredRedaction", "required_only", required_redaction())?;
    round_trip("StoredRedaction", "fully_populated", full_redaction())?;

    round_trip("StoredReflexStep", "empty", empty_reflex_step())?;
    round_trip("StoredReflexStep", "required_only", required_reflex_step())?;
    round_trip("StoredReflexStep", "fully_populated", full_reflex_step())?;

    round_trip(
        "StoredProfileHistoryEntry",
        "empty",
        empty_profile_history_entry(),
    )?;
    round_trip(
        "StoredProfileHistoryEntry",
        "required_only",
        required_profile_history_entry(),
    )?;
    round_trip(
        "StoredProfileHistoryEntry",
        "fully_populated",
        full_profile_history_entry(),
    )?;

    Ok(())
}

#[test]
fn stored_type_json_snapshots() -> Result<(), Box<dyn std::error::Error>> {
    insta::assert_json_snapshot!(
        "stored_event_round_trip",
        round_trip("StoredEvent", "snapshot", full_event())?
    );
    insta::assert_json_snapshot!(
        "stored_observation_round_trip",
        round_trip("StoredObservation", "snapshot", full_observation())?
    );
    insta::assert_json_snapshot!(
        "stored_reflex_audit_round_trip",
        round_trip("StoredReflexAudit", "snapshot", full_reflex_audit())?
    );
    insta::assert_json_snapshot!(
        "stored_session_round_trip",
        round_trip("StoredSession", "snapshot", full_session())?
    );
    insta::assert_json_snapshot!(
        "stored_redaction_round_trip",
        round_trip("StoredRedaction", "snapshot", full_redaction())?
    );
    insta::assert_json_snapshot!(
        "stored_reflex_step_round_trip",
        round_trip("StoredReflexStep", "snapshot", full_reflex_step())?
    );
    insta::assert_json_snapshot!(
        "stored_profile_history_entry_round_trip",
        round_trip(
            "StoredProfileHistoryEntry",
            "snapshot",
            full_profile_history_entry(),
        )?
    );

    Ok(())
}

#[test]
fn stored_types_reject_unknown_fields() -> Result<(), Box<dyn std::error::Error>> {
    reject_unknown_field("StoredEvent", full_event())?;
    reject_unknown_field("StoredObservation", full_observation())?;
    reject_unknown_field("StoredReflexAudit", full_reflex_audit())?;
    reject_unknown_field("StoredSession", full_session())?;
    Ok(())
}

#[test]
fn stored_types_proptest_json_round_trip_is_deterministic() -> Result<(), Box<dyn std::error::Error>>
{
    assert_strategy_round_trips("StoredEvent", stored_event_strategy())?;
    assert_strategy_round_trips("StoredObservation", stored_observation_strategy())?;
    assert_strategy_round_trips("StoredReflexAudit", stored_reflex_audit_strategy())?;
    assert_strategy_round_trips("StoredSession", stored_session_strategy())?;
    assert_strategy_round_trips("StoredRedaction", stored_redaction_strategy())?;
    assert_strategy_round_trips("StoredReflexStep", stored_reflex_step_strategy())?;
    assert_strategy_round_trips(
        "StoredProfileHistoryEntry",
        stored_profile_history_entry_strategy(),
    )?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn round_trip<T>(type_name: &str, edge: &str, value: T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Clone + Debug + PartialEq + Serialize + DeserializeOwned + 'static,
{
    let before = serde_json::to_value(value.clone())?;
    println!("source_of_truth=json_stored_record type={type_name} edge={edge} before={before}");
    let parsed = serde_json::from_value::<T>(before)?;
    let after = serde_json::to_value(&parsed)?;
    println!(
        "source_of_truth=json_stored_record type={type_name} edge={edge} after={after} final_value={after}"
    );
    assert_eq!(parsed, value);
    Ok(parsed)
}

#[allow(clippy::needless_pass_by_value)]
fn reject_unknown_field<T>(type_name: &str, value: T) -> Result<(), Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    let mut json = serde_json::to_value(value)?;
    let serde_json::Value::Object(map) = &mut json else {
        return Err(format!("{type_name} did not serialize to an object").into());
    };
    map.insert(
        "unknown_field".to_owned(),
        serde_json::Value::String("must reject".to_owned()),
    );
    println!("source_of_truth=json_unknown_field type={type_name} before={json}");
    let rejected = serde_json::from_value::<T>(json).is_err();
    println!(
        "source_of_truth=json_unknown_field type={type_name} after=rejected:{rejected} final_value={rejected}"
    );
    assert!(rejected);
    Ok(())
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

    println!("source_of_truth=json_stored_record_proptest type={type_name} before=cases:1000");
    runner.run(&strategy, |value| {
        let json = serde_json::to_value(value.clone())?;
        let parsed = serde_json::from_value::<T>(json)?;
        prop_assert_eq!(parsed, value);
        Ok(())
    })?;
    println!(
        "source_of_truth=json_stored_record_proptest type={type_name} after=cases:1000 final_value=all_round_tripped"
    );
    Ok(())
}

fn empty_event() -> StoredEvent {
    StoredEvent {
        schema_version: SCHEMA_VERSION,
        event_id: String::new(),
        ts_ns: 0,
        session_id: None,
        source: EventSource::System,
        kind: String::new(),
        data: serde_json::Value::Null,
        window_id: None,
        element_id: None,
        redacted: false,
        redactions: Vec::new(),
    }
}

fn required_event() -> StoredEvent {
    StoredEvent {
        event_id: "event-1".to_owned(),
        ts_ns: 1,
        source: EventSource::ActionEmitter,
        kind: "action_completed".to_owned(),
        ..empty_event()
    }
}

fn full_event() -> StoredEvent {
    StoredEvent {
        schema_version: SCHEMA_VERSION,
        event_id: "event-full".to_owned(),
        ts_ns: 42,
        session_id: Some("session-full".to_owned()),
        source: EventSource::PerceptionHud,
        kind: "hud_value_changed".to_owned(),
        data: serde_json::json!({"field":"hp","new":20}),
        window_id: Some(0x1234),
        element_id: Some(element_id(0x1234, "0102")),
        redacted: true,
        redactions: vec![full_redaction()],
    }
}

fn empty_observation() -> StoredObservation {
    StoredObservation {
        schema_version: SCHEMA_VERSION,
        observation_id: String::new(),
        ts_ns: 0,
        session_id: None,
        mode: PerceptionMode::Auto,
        foreground: foreground(""),
        focused: None,
        elements: Vec::new(),
        entities: Vec::new(),
        hud: HudReadings::default(),
        audio: AudioContext::default(),
        recent_events: Vec::new(),
        clipboard_summary: None,
        fs_recent: Vec::new(),
        diagnostics: diagnostics(),
        reason: String::new(),
        redacted: false,
        redactions: Vec::new(),
    }
}

fn required_observation() -> StoredObservation {
    StoredObservation {
        observation_id: "obs-1".to_owned(),
        ts_ns: 1,
        reason: "1hz_sample".to_owned(),
        ..empty_observation()
    }
}

fn full_observation() -> StoredObservation {
    let at = fixed_time(10);
    let mut hud = HudReadings::default();
    hud.by_name.insert(
        "hp".to_owned(),
        HudReading {
            raw_text: "20".to_owned(),
            parsed: HudValue::Number(20.0),
            confidence: 0.97,
            stale_ms: 0,
        },
    );

    StoredObservation {
        schema_version: SCHEMA_VERSION,
        observation_id: "obs-full".to_owned(),
        ts_ns: 10,
        session_id: Some("session-full".to_owned()),
        mode: PerceptionMode::Hybrid,
        foreground: foreground("notepad.exe"),
        focused: Some(focused_element()),
        elements: vec![accessible_node()],
        entities: vec![DetectedEntity {
            entity_id: entity_id(7),
            track_id: 7,
            class_label: "cursor".to_owned(),
            bbox: Rect {
                x: 120,
                y: 130,
                w: 16,
                h: 24,
            },
            confidence: 0.75,
            first_seen_at: at,
            last_seen_at: at,
            velocity_px_per_s: Some((0.0, 0.0)),
        }],
        hud,
        audio: AudioContext {
            rms_db: -42.0,
            vad_speech_recent: false,
            recent_events: vec![AudioEvent {
                at,
                kind: "loud_transient".to_owned(),
                azimuth_deg: Some(15.0),
                confidence: 0.8,
            }],
            direction_estimate: Some(DirectionEstimate {
                azimuth_deg: 15.0,
                confidence: 0.8,
            }),
        },
        recent_events: vec![EventSummary {
            seq: 1,
            at,
            source: EventSource::PerceptionHud,
            kind: "hud_value_changed".to_owned(),
            data_excerpt: serde_json::json!({"field":"hp"}),
        }],
        clipboard_summary: Some(ClipboardSummary {
            formats: vec!["text/plain".to_owned()],
            text_len: Some(5),
            text_excerpt: Some("hello".to_owned()),
            redacted: false,
        }),
        fs_recent: vec![FsEvent {
            at,
            path: "C:\\Users\\owner\\note.txt".to_owned(),
            kind: FsEventKind::Modified,
            size_bytes: Some(5),
        }],
        diagnostics: diagnostics(),
        reason: "before_action".to_owned(),
        redacted: true,
        redactions: vec![full_redaction()],
    }
}

fn empty_reflex_audit() -> StoredReflexAudit {
    StoredReflexAudit {
        schema_version: SCHEMA_VERSION,
        audit_id: String::new(),
        reflex_id: String::new(),
        ts_ns: 0,
        status: ReflexState::Active,
        event_id: None,
        steps: Vec::new(),
        error_code: None,
        details: serde_json::Value::Null,
        redacted: false,
        redactions: Vec::new(),
    }
}

fn required_reflex_audit() -> StoredReflexAudit {
    StoredReflexAudit {
        audit_id: "audit-1".to_owned(),
        reflex_id: "reflex-1".to_owned(),
        ts_ns: 1,
        ..empty_reflex_audit()
    }
}

fn full_reflex_audit() -> StoredReflexAudit {
    StoredReflexAudit {
        schema_version: SCHEMA_VERSION,
        audit_id: "audit-full".to_owned(),
        reflex_id: "reflex-full".to_owned(),
        ts_ns: 42,
        status: ReflexState::Starved,
        event_id: Some("event-full".to_owned()),
        steps: vec![full_reflex_step()],
        error_code: Some("REFLEX_STARVED".to_owned()),
        details: serde_json::json!({"lost_for_ms": 2000}),
        redacted: true,
        redactions: vec![full_redaction()],
    }
}

fn empty_session() -> StoredSession {
    StoredSession {
        schema_version: SCHEMA_VERSION,
        session_id: String::new(),
        started_at: fixed_time(0),
        ended_at: None,
        transport: String::new(),
        client: None,
        mode: PerceptionMode::Auto,
        active_profile: None,
        profile_history: Vec::new(),
        redacted: false,
        redactions: Vec::new(),
    }
}

fn required_session() -> StoredSession {
    StoredSession {
        session_id: "session-1".to_owned(),
        transport: "stdio".to_owned(),
        ..empty_session()
    }
}

fn full_session() -> StoredSession {
    StoredSession {
        schema_version: SCHEMA_VERSION,
        session_id: "session-full".to_owned(),
        started_at: fixed_time(10),
        ended_at: Some(fixed_time(20)),
        transport: "http".to_owned(),
        client: Some("claude-desktop/0.4.2".to_owned()),
        mode: PerceptionMode::Hybrid,
        active_profile: Some("notepad".to_owned()),
        profile_history: vec![full_profile_history_entry()],
        redacted: true,
        redactions: vec![full_redaction()],
    }
}

fn empty_redaction() -> StoredRedaction {
    StoredRedaction {
        kind: String::new(),
        offset: 0,
        len: 0,
    }
}

fn required_redaction() -> StoredRedaction {
    StoredRedaction {
        kind: "email".to_owned(),
        offset: 0,
        len: 5,
    }
}

fn full_redaction() -> StoredRedaction {
    StoredRedaction {
        kind: "secret".to_owned(),
        offset: 12,
        len: 8,
    }
}

fn empty_reflex_step() -> StoredReflexStep {
    StoredReflexStep {
        index: 0,
        action: key_press_action("space"),
        status: String::new(),
        error_code: None,
    }
}

fn required_reflex_step() -> StoredReflexStep {
    StoredReflexStep {
        status: "queued".to_owned(),
        ..empty_reflex_step()
    }
}

fn full_reflex_step() -> StoredReflexStep {
    StoredReflexStep {
        index: 1,
        action: Action::MouseButton {
            button: MouseButton::Left,
            action: ButtonAction::Press,
            hold_ms: 16,
            backend: Backend::Software,
        },
        status: "completed".to_owned(),
        error_code: None,
    }
}

fn empty_profile_history_entry() -> StoredProfileHistoryEntry {
    StoredProfileHistoryEntry {
        profile_id: String::new(),
        activated_at: fixed_time(0),
        reason: String::new(),
    }
}

fn required_profile_history_entry() -> StoredProfileHistoryEntry {
    StoredProfileHistoryEntry {
        profile_id: "notepad".to_owned(),
        activated_at: fixed_time(1),
        reason: "manual".to_owned(),
    }
}

fn full_profile_history_entry() -> StoredProfileHistoryEntry {
    StoredProfileHistoryEntry {
        profile_id: "vscode".to_owned(),
        activated_at: fixed_time(2),
        reason: "window_match".to_owned(),
    }
}

fn foreground(process_name: &str) -> ForegroundContext {
    ForegroundContext {
        hwnd: 0x1234,
        pid: 42,
        process_name: process_name.to_owned(),
        process_path: "C:\\Windows\\System32\\notepad.exe".to_owned(),
        window_title: "Untitled - Notepad".to_owned(),
        window_bounds: Rect {
            x: 10,
            y: 20,
            w: 800,
            h: 600,
        },
        monitor_index: 0,
        dpi_scale: 1.0,
        profile_id: Some("notepad".to_owned()),
        steam_appid: None,
        is_fullscreen: false,
        is_dwm_composed: true,
    }
}

fn focused_element() -> FocusedElement {
    FocusedElement {
        element_id: element_id(0x1234, "0102"),
        name: "Editor".to_owned(),
        role: "document".to_owned(),
        automation_id: Some("15".to_owned()),
        bbox: Rect {
            x: 20,
            y: 30,
            w: 700,
            h: 500,
        },
        enabled: true,
        patterns: vec![UiaPattern::Value, UiaPattern::Text],
        value: Some("hello".to_owned()),
        selected_text: None,
    }
}

fn accessible_node() -> AccessibleNode {
    AccessibleNode {
        element_id: element_id(0x1234, "0102"),
        parent: None,
        name: "Editor".to_owned(),
        role: "document".to_owned(),
        automation_id: Some("15".to_owned()),
        bbox: Rect {
            x: 20,
            y: 30,
            w: 700,
            h: 500,
        },
        enabled: true,
        focused: true,
        patterns: vec![UiaPattern::Value],
        children_count: 0,
        depth: 0,
    }
}

fn diagnostics() -> ObservationDiagnostics {
    ObservationDiagnostics {
        assembled_in_ms: 1.5,
        sensor_latency_ms: BTreeMap::new(),
        a11y_enabled: true,
        pixel_enabled: true,
        audio_enabled: false,
        a11y_status: SensorStatus::Healthy,
        capture_status: SensorStatus::Healthy,
        detection_status: SensorStatus::Unavailable,
        audio_status: SensorStatus::Disabled,
        elements_truncated: false,
        entities_truncated: false,
        size_bytes: 256,
        size_estimate_tokens: 64,
    }
}

fn key_press_action(key: &str) -> Action {
    Action::KeyPress {
        key: Key {
            code: KeyCode::Named {
                value: key.to_owned(),
            },
            use_scancode: false,
        },
        hold_ms: 30,
        backend: Backend::Software,
    }
}

fn fixed_time(offset_seconds: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from(std::time::SystemTime::UNIX_EPOCH) + Duration::seconds(offset_seconds)
}

fn small_string() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,8}".prop_map(|value| value)
}

fn json_value_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        (0_i64..10_000).prop_map(|value| serde_json::json!(value)),
        small_string().prop_map(serde_json::Value::String),
    ]
}

fn event_source_strategy() -> impl Strategy<Value = EventSource> {
    prop_oneof![
        Just(EventSource::System),
        Just(EventSource::ActionEmitter),
        Just(EventSource::PerceptionHud),
        Just(EventSource::Reflex),
    ]
}

fn perception_mode_strategy() -> impl Strategy<Value = PerceptionMode> {
    prop_oneof![
        Just(PerceptionMode::A11yOnly),
        Just(PerceptionMode::PixelOnly),
        Just(PerceptionMode::Hybrid),
        Just(PerceptionMode::Auto),
    ]
}

fn sensor_status_strategy() -> impl Strategy<Value = SensorStatus> {
    prop_oneof![
        Just(SensorStatus::Healthy),
        Just(SensorStatus::Disabled),
        Just(SensorStatus::Unavailable),
        (0.0_f32..250.0).prop_map(|last_p99_ms| SensorStatus::DegradedLatency { last_p99_ms }),
        small_string().prop_map(|reason_code| SensorStatus::DegradedSensorFailed { reason_code }),
    ]
}

fn instant_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    (0_i64..86_400).prop_map(fixed_time)
}

fn element_id_strategy() -> impl Strategy<Value = ElementId> {
    (1_i64..10_000, 1_u32..10_000).prop_map(|(hwnd, runtime)| {
        let runtime_id_hex = format!("{runtime:x}");
        element_id(hwnd, &runtime_id_hex)
    })
}

fn redactions_strategy() -> impl Strategy<Value = Vec<StoredRedaction>> {
    prop::collection::vec(stored_redaction_strategy(), 0..3)
}

fn stored_redaction_strategy() -> impl Strategy<Value = StoredRedaction> {
    (small_string(), 0_u32..256, 0_u32..64).prop_map(|(kind, offset, len)| StoredRedaction {
        kind,
        offset,
        len,
    })
}

fn rect_strategy() -> impl Strategy<Value = Rect> {
    (
        -10_000_i32..10_000,
        -10_000_i32..10_000,
        1_i32..2_000,
        1_i32..2_000,
    )
        .prop_map(|(x, y, w, h)| Rect { x, y, w, h })
}

fn foreground_strategy() -> impl Strategy<Value = ForegroundContext> {
    (
        1_i64..10_000,
        1_u32..50_000,
        small_string(),
        small_string(),
        rect_strategy(),
        0_u32..4,
        1.0_f32..3.0,
        prop::option::of(small_string()),
        any::<bool>(),
    )
        .prop_map(
            |(
                hwnd,
                pid,
                process_name,
                window_title,
                window_bounds,
                monitor_index,
                dpi_scale,
                profile_id,
                is_fullscreen,
            )| ForegroundContext {
                hwnd,
                pid,
                process_path: format!("C:\\Apps\\{process_name}.exe"),
                process_name,
                window_title,
                window_bounds,
                monitor_index,
                dpi_scale,
                profile_id,
                steam_appid: None,
                is_fullscreen,
                is_dwm_composed: true,
            },
        )
}

fn focused_element_strategy() -> impl Strategy<Value = FocusedElement> {
    (
        element_id_strategy(),
        small_string(),
        small_string(),
        prop::option::of(small_string()),
        rect_strategy(),
        any::<bool>(),
        prop::option::of(small_string()),
    )
        .prop_map(
            |(element_id, name, role, automation_id, bbox, enabled, value)| FocusedElement {
                element_id,
                name,
                role,
                automation_id,
                bbox,
                enabled,
                patterns: vec![UiaPattern::Value],
                value,
                selected_text: None,
            },
        )
}

fn diagnostics_strategy() -> impl Strategy<Value = ObservationDiagnostics> {
    (
        0.0_f32..50.0,
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        sensor_status_strategy(),
        sensor_status_strategy(),
        sensor_status_strategy(),
        sensor_status_strategy(),
        0_u32..10_000,
        0_u32..10_000,
    )
        .prop_map(
            |(
                assembled_in_ms,
                a11y_enabled,
                pixel_enabled,
                audio_enabled,
                a11y_status,
                capture_status,
                detection_status,
                audio_status,
                size_bytes,
                size_estimate_tokens,
            )| ObservationDiagnostics {
                assembled_in_ms,
                sensor_latency_ms: BTreeMap::new(),
                a11y_enabled,
                pixel_enabled,
                audio_enabled,
                a11y_status,
                capture_status,
                detection_status,
                audio_status,
                elements_truncated: false,
                entities_truncated: false,
                size_bytes,
                size_estimate_tokens,
            },
        )
}

fn stored_event_strategy() -> impl Strategy<Value = StoredEvent> {
    (
        small_string(),
        0_u64..1_000_000,
        prop::option::of(small_string()),
        event_source_strategy(),
        small_string(),
        json_value_strategy(),
        prop::option::of(1_i64..10_000),
        prop::option::of(element_id_strategy()),
        any::<bool>(),
        redactions_strategy(),
    )
        .prop_map(
            |(
                event_id,
                ts_ns,
                session_id,
                source,
                kind,
                data,
                window_id,
                element_id,
                redacted,
                redactions,
            )| StoredEvent {
                schema_version: SCHEMA_VERSION,
                event_id,
                ts_ns,
                session_id,
                source,
                kind,
                data,
                window_id,
                element_id,
                redacted,
                redactions,
            },
        )
}

fn stored_observation_strategy() -> impl Strategy<Value = StoredObservation> {
    (
        small_string(),
        0_u64..1_000_000,
        prop::option::of(small_string()),
        perception_mode_strategy(),
        foreground_strategy(),
        prop::option::of(focused_element_strategy()),
        diagnostics_strategy(),
        small_string(),
        any::<bool>(),
        redactions_strategy(),
    )
        .prop_map(
            |(
                observation_id,
                ts_ns,
                session_id,
                mode,
                foreground,
                focused,
                diagnostics,
                reason,
                redacted,
                redactions,
            )| StoredObservation {
                schema_version: SCHEMA_VERSION,
                observation_id,
                ts_ns,
                session_id,
                mode,
                foreground,
                focused,
                elements: Vec::new(),
                entities: Vec::new(),
                hud: HudReadings::default(),
                audio: AudioContext::default(),
                recent_events: Vec::new(),
                clipboard_summary: None,
                fs_recent: Vec::new(),
                diagnostics,
                reason,
                redacted,
                redactions,
            },
        )
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        (small_string(), 0_u32..250).prop_map(|(value, hold_ms)| Action::KeyPress {
            key: Key {
                code: KeyCode::Named { value },
                use_scancode: false,
            },
            hold_ms,
            backend: Backend::Software,
        }),
        (-500.0_f32..500.0, -500.0_f32..500.0).prop_map(|(dx, dy)| Action::MouseMoveRelative {
            dx,
            dy,
            backend: Backend::Software,
        }),
    ]
}

fn stored_reflex_step_strategy() -> impl Strategy<Value = StoredReflexStep> {
    (
        0_u32..32,
        action_strategy(),
        small_string(),
        prop::option::of(small_string()),
    )
        .prop_map(|(index, action, status, error_code)| StoredReflexStep {
            index,
            action,
            status,
            error_code,
        })
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

fn stored_reflex_audit_strategy() -> impl Strategy<Value = StoredReflexAudit> {
    (
        small_string(),
        small_string(),
        0_u64..1_000_000,
        reflex_state_strategy(),
        prop::option::of(small_string()),
        prop::collection::vec(stored_reflex_step_strategy(), 0..3),
        prop::option::of(small_string()),
        json_value_strategy(),
        any::<bool>(),
        redactions_strategy(),
    )
        .prop_map(
            |(
                audit_id,
                reflex_id,
                ts_ns,
                status,
                event_id,
                steps,
                error_code,
                details,
                redacted,
                redactions,
            )| StoredReflexAudit {
                schema_version: SCHEMA_VERSION,
                audit_id,
                reflex_id,
                ts_ns,
                status,
                event_id,
                steps,
                error_code,
                details,
                redacted,
                redactions,
            },
        )
}

fn stored_profile_history_entry_strategy() -> impl Strategy<Value = StoredProfileHistoryEntry> {
    (small_string(), instant_strategy(), small_string()).prop_map(
        |(profile_id, activated_at, reason)| StoredProfileHistoryEntry {
            profile_id,
            activated_at,
            reason,
        },
    )
}

fn stored_session_strategy() -> impl Strategy<Value = StoredSession> {
    (
        small_string(),
        instant_strategy(),
        prop::option::of(instant_strategy()),
        small_string(),
        prop::option::of(small_string()),
        perception_mode_strategy(),
        prop::option::of(small_string()),
        prop::collection::vec(stored_profile_history_entry_strategy(), 0..3),
        any::<bool>(),
        redactions_strategy(),
    )
        .prop_map(
            |(
                session_id,
                started_at,
                ended_at,
                transport,
                client,
                mode,
                active_profile,
                profile_history,
                redacted,
                redactions,
            )| StoredSession {
                schema_version: SCHEMA_VERSION,
                session_id,
                started_at,
                ended_at,
                transport,
                client,
                mode,
                active_profile,
                profile_history,
                redacted,
                redactions,
            },
        )
}
