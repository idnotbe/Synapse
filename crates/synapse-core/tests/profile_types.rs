#![allow(clippy::missing_const_for_fn)]

use std::{collections::BTreeMap, fmt::Debug};

use proptest::{
    prelude::*,
    test_runner::{Config, TestRng, TestRunner},
};
use serde::{Serialize, de::DeserializeOwned};
use synapse_core::{
    Backend, DataPredicate, EventExtension, EventFilter, EventSource, HudExtractor, HudFieldSpec,
    HudParser, HudRegion, OcrBackend, PerceptionMode, Profile, ProfileBackends, ProfileCapture,
    ProfileCaptureTarget, ProfileDetection, ProfileMatch, ProfileOcr, ProfileUseScope, WindowEdge,
};

#[test]
fn profile_type_edge_round_trips_with_fsv() -> Result<(), Box<dyn std::error::Error>> {
    round_trip("Profile", "empty", empty_profile("empty"))?;
    round_trip("Profile", "required_only", required_profile("required"))?;
    round_trip("Profile", "fully_populated", full_profile())?;

    round_trip("ProfileMatch", "empty", empty_profile_match())?;
    round_trip(
        "ProfileMatch",
        "required_only",
        exe_profile_match("notepad.exe"),
    )?;
    round_trip("ProfileMatch", "fully_populated", full_profile_match())?;

    round_trip("ProfileCapture", "empty", foreground_capture())?;
    round_trip("ProfileCapture", "required_only", primary_monitor_capture())?;
    round_trip(
        "ProfileCapture",
        "fully_populated",
        monitor_index_capture(2),
    )?;

    round_trip("ProfileDetection", "empty", disabled_detection())?;
    round_trip("ProfileDetection", "required_only", minimal_detection())?;
    round_trip("ProfileDetection", "fully_populated", full_detection())?;

    round_trip("ProfileOcr", "empty", empty_ocr())?;
    round_trip("ProfileOcr", "required_only", winrt_ocr())?;
    round_trip("ProfileOcr", "fully_populated", full_ocr())?;

    round_trip("HudFieldSpec", "empty", minimal_hud_field("empty"))?;
    round_trip(
        "HudFieldSpec",
        "required_only",
        minimal_hud_field("required"),
    )?;
    round_trip("HudFieldSpec", "fully_populated", full_hud_field())?;

    round_trip("ProfileBackends", "empty", software_backends())?;
    round_trip("ProfileBackends", "required_only", software_backends())?;
    round_trip("ProfileBackends", "fully_populated", mixed_backends())?;

    round_trip("EventExtension", "empty", minimal_event_extension("empty"))?;
    round_trip(
        "EventExtension",
        "required_only",
        minimal_event_extension("required"),
    )?;
    round_trip("EventExtension", "fully_populated", full_event_extension())?;

    Ok(())
}

#[test]
fn profile_type_json_snapshots() -> Result<(), Box<dyn std::error::Error>> {
    insta::assert_json_snapshot!(
        "profile_round_trip",
        round_trip("Profile", "snapshot", full_profile())?
    );
    insta::assert_json_snapshot!(
        "profile_match_round_trip",
        round_trip("ProfileMatch", "snapshot", full_profile_match())?
    );
    insta::assert_json_snapshot!(
        "profile_capture_round_trip",
        round_trip("ProfileCapture", "snapshot", monitor_index_capture(2))?
    );
    insta::assert_json_snapshot!(
        "profile_capture_target_round_trip",
        round_trip(
            "ProfileCaptureTarget",
            "snapshot",
            ProfileCaptureTarget::MonitorIndex { index: 2 },
        )?
    );
    insta::assert_json_snapshot!(
        "profile_detection_round_trip",
        round_trip("ProfileDetection", "snapshot", full_detection())?
    );
    insta::assert_json_snapshot!(
        "profile_ocr_round_trip",
        round_trip("ProfileOcr", "snapshot", full_ocr())?
    );
    insta::assert_json_snapshot!(
        "hud_field_spec_round_trip",
        round_trip("HudFieldSpec", "snapshot", full_hud_field())?
    );
    insta::assert_json_snapshot!(
        "hud_region_round_trip",
        round_trip("HudRegion", "snapshot", anchored_region())?
    );
    insta::assert_json_snapshot!(
        "window_edge_round_trip",
        round_trip("WindowEdge", "snapshot", WindowEdge::BottomLeft)?
    );
    insta::assert_json_snapshot!(
        "hud_extractor_round_trip",
        round_trip("HudExtractor", "snapshot", full_extractor())?
    );
    insta::assert_json_snapshot!(
        "hud_parser_round_trip",
        round_trip("HudParser", "snapshot", full_parser())?
    );
    insta::assert_json_snapshot!(
        "profile_backends_round_trip",
        round_trip("ProfileBackends", "snapshot", mixed_backends())?
    );
    insta::assert_json_snapshot!(
        "event_extension_round_trip",
        round_trip("EventExtension", "snapshot", full_event_extension())?
    );
    insta::assert_json_snapshot!(
        "profile_use_scope_round_trip",
        round_trip(
            "ProfileUseScope",
            "snapshot",
            ProfileUseScope::OperatorOwnedTest,
        )?
    );

    Ok(())
}

#[test]
fn profile_types_proptest_json_round_trip_is_deterministic()
-> Result<(), Box<dyn std::error::Error>> {
    assert_strategy_round_trips("Profile", profile_strategy())?;
    assert_strategy_round_trips("ProfileMatch", profile_match_strategy())?;
    assert_strategy_round_trips("ProfileCapture", profile_capture_strategy())?;
    assert_strategy_round_trips("ProfileCaptureTarget", profile_capture_target_strategy())?;
    assert_strategy_round_trips("ProfileDetection", profile_detection_strategy())?;
    assert_strategy_round_trips("ProfileOcr", profile_ocr_strategy())?;
    assert_strategy_round_trips("HudFieldSpec", hud_field_spec_strategy())?;
    assert_strategy_round_trips("HudRegion", hud_region_strategy())?;
    assert_strategy_round_trips("WindowEdge", window_edge_strategy())?;
    assert_strategy_round_trips("HudExtractor", hud_extractor_strategy())?;
    assert_strategy_round_trips("HudParser", hud_parser_strategy())?;
    assert_strategy_round_trips("ProfileBackends", profile_backends_strategy())?;
    assert_strategy_round_trips("EventExtension", event_extension_strategy())?;
    assert_strategy_round_trips("ProfileUseScope", profile_use_scope_strategy())?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn round_trip<T>(type_name: &str, edge: &str, value: T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Clone + Debug + PartialEq + Serialize + DeserializeOwned + 'static,
{
    let before = serde_json::to_value(value.clone())?;
    println!("source_of_truth=serde_round_trip type={type_name} edge={edge} before={before}");
    let parsed = serde_json::from_value::<T>(before)?;
    let after = serde_json::to_value(&parsed)?;
    println!(
        "source_of_truth=serde_round_trip type={type_name} edge={edge} after={after} final_value={after}"
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

    println!("source_of_truth=serde_round_trip_proptest type={type_name} before=cases:1000");
    runner.run(&strategy, |value| {
        let json = serde_json::to_value(value.clone())?;
        let parsed = serde_json::from_value::<T>(json)?;
        prop_assert_eq!(parsed, value);
        Ok(())
    })?;
    println!(
        "source_of_truth=serde_round_trip_proptest type={type_name} after=cases:1000 final_value=all_round_tripped"
    );
    Ok(())
}

fn empty_profile(id: &str) -> Profile {
    Profile {
        id: id.to_owned(),
        label: "Empty Profile".to_owned(),
        version: "1.0.0".to_owned(),
        use_scope: ProfileUseScope::Unknown,
        matches: Vec::new(),
        mode: PerceptionMode::Auto,
        capture: foreground_capture(),
        detection: disabled_detection(),
        ocr: empty_ocr(),
        hud: Vec::new(),
        keymap: BTreeMap::new(),
        backends: software_backends(),
        event_extensions: Vec::new(),
    }
}

fn required_profile(id: &str) -> Profile {
    let mut profile = empty_profile(id);
    "Required Profile".clone_into(&mut profile.label);
    profile.matches = vec![exe_profile_match("notepad.exe")];
    profile.mode = PerceptionMode::A11yOnly;
    profile
}

fn full_profile() -> Profile {
    let mut keymap = BTreeMap::new();
    keymap.insert("attack".to_owned(), "lmb".to_owned());
    keymap.insert("inventory".to_owned(), "e".to_owned());

    Profile {
        id: "minecraft.java".to_owned(),
        label: "Minecraft Java Edition".to_owned(),
        version: "1.0.0".to_owned(),
        use_scope: ProfileUseScope::SinglePlayer,
        matches: vec![full_profile_match()],
        mode: PerceptionMode::PixelOnly,
        capture: monitor_index_capture(1),
        detection: full_detection(),
        ocr: full_ocr(),
        hud: vec![full_hud_field()],
        keymap,
        backends: mixed_backends(),
        event_extensions: vec![full_event_extension()],
    }
}

fn empty_profile_match() -> ProfileMatch {
    ProfileMatch {
        exe: None,
        title_regex: None,
        steam_appid: None,
        window_class: None,
        process_args: Vec::new(),
    }
}

fn exe_profile_match(exe: &str) -> ProfileMatch {
    ProfileMatch {
        exe: Some(exe.to_owned()),
        ..empty_profile_match()
    }
}

fn full_profile_match() -> ProfileMatch {
    ProfileMatch {
        exe: Some("javaw.exe".to_owned()),
        title_regex: Some("Minecraft\\* [0-9]".to_owned()),
        steam_appid: Some(12_345),
        window_class: Some("LWJGL".to_owned()),
        process_args: vec!["--demo".to_owned()],
    }
}

fn foreground_capture() -> ProfileCapture {
    ProfileCapture {
        target: ProfileCaptureTarget::ForegroundWindow,
        min_update_interval_ms: 100,
        cursor_visible: true,
    }
}

fn primary_monitor_capture() -> ProfileCapture {
    ProfileCapture {
        target: ProfileCaptureTarget::PrimaryMonitor,
        min_update_interval_ms: 33,
        cursor_visible: true,
    }
}

fn monitor_index_capture(index: u32) -> ProfileCapture {
    ProfileCapture {
        target: ProfileCaptureTarget::MonitorIndex { index },
        min_update_interval_ms: 16,
        cursor_visible: false,
    }
}

fn disabled_detection() -> ProfileDetection {
    ProfileDetection {
        model_id: None,
        classes_of_interest: Vec::new(),
        confidence_threshold: 0.0,
        max_detections: 0,
    }
}

fn minimal_detection() -> ProfileDetection {
    ProfileDetection {
        model_id: Some("none".to_owned()),
        classes_of_interest: Vec::new(),
        confidence_threshold: 0.0,
        max_detections: 0,
    }
}

fn full_detection() -> ProfileDetection {
    ProfileDetection {
        model_id: Some("yolov10n_general".to_owned()),
        classes_of_interest: vec!["player".to_owned(), "creeper".to_owned()],
        confidence_threshold: 0.45,
        max_detections: 32,
    }
}

fn empty_ocr() -> ProfileOcr {
    ProfileOcr {
        default_backend: OcrBackend::Auto,
        regions: Vec::new(),
        parser_config: BTreeMap::new(),
    }
}

fn winrt_ocr() -> ProfileOcr {
    ProfileOcr {
        default_backend: OcrBackend::Winrt,
        regions: Vec::new(),
        parser_config: BTreeMap::new(),
    }
}

fn full_ocr() -> ProfileOcr {
    let mut parser_config = BTreeMap::new();
    parser_config.insert("language".to_owned(), "en".to_owned());
    parser_config.insert("normalize_whitespace".to_owned(), "true".to_owned());

    ProfileOcr {
        default_backend: OcrBackend::Crnn,
        regions: vec![anchored_region()],
        parser_config,
    }
}

fn minimal_hud_field(name: &str) -> HudFieldSpec {
    HudFieldSpec {
        name: name.to_owned(),
        region: HudRegion::Absolute {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        extractor: HudExtractor::WinrtOcr,
        parser: HudParser::Number,
    }
}

fn full_hud_field() -> HudFieldSpec {
    HudFieldSpec {
        name: "hp_hearts".to_owned(),
        region: anchored_region(),
        extractor: full_extractor(),
        parser: full_parser(),
    }
}

fn anchored_region() -> HudRegion {
    HudRegion::AnchoredToEdge {
        edge: WindowEdge::BottomLeft,
        x_offset: 220,
        y_offset: -50,
        w: 180,
        h: 18,
    }
}

fn full_extractor() -> HudExtractor {
    HudExtractor::TemplateMatch {
        templates: vec![
            "hearts/full.png".to_owned(),
            "hearts/half.png".to_owned(),
            "hearts/empty.png".to_owned(),
        ],
    }
}

fn full_parser() -> HudParser {
    HudParser::Regex {
        pattern: r"([0-9]+)/[0-9]+".to_owned(),
        group: 1,
    }
}

fn software_backends() -> ProfileBackends {
    ProfileBackends {
        default: Backend::Software,
        keyboard_default: Backend::Software,
        mouse_default: Backend::Software,
        pad_default: Backend::Software,
    }
}

fn mixed_backends() -> ProfileBackends {
    ProfileBackends {
        default: Backend::Auto,
        keyboard_default: Backend::Software,
        mouse_default: Backend::Hardware,
        pad_default: Backend::Vigem,
    }
}

fn minimal_event_extension(name: &str) -> EventExtension {
    EventExtension {
        name: name.to_owned(),
        from_filter: EventFilter::All,
        emits_kind: format!("{name}-event"),
    }
}

fn full_event_extension() -> EventExtension {
    EventExtension {
        name: "creeper_nearby".to_owned(),
        from_filter: EventFilter::And {
            args: vec![
                EventFilter::Kind {
                    kind: "entity-appeared".to_owned(),
                },
                EventFilter::Data {
                    path: "/class_label".to_owned(),
                    predicate: DataPredicate::Eq {
                        value: serde_json::json!("creeper"),
                    },
                },
                EventFilter::Data {
                    path: "/bbox/w".to_owned(),
                    predicate: DataPredicate::Gt {
                        value: serde_json::json!(80),
                    },
                },
            ],
        },
        emits_kind: "creeper-imminent".to_owned(),
    }
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

fn perception_mode_strategy() -> impl Strategy<Value = PerceptionMode> {
    prop_oneof![
        Just(PerceptionMode::A11yOnly),
        Just(PerceptionMode::PixelOnly),
        Just(PerceptionMode::Hybrid),
        Just(PerceptionMode::Auto),
    ]
}

fn profile_use_scope_strategy() -> impl Strategy<Value = ProfileUseScope> {
    prop_oneof![
        Just(ProfileUseScope::Productivity),
        Just(ProfileUseScope::SinglePlayer),
        Just(ProfileUseScope::OperatorOwnedTest),
        Just(ProfileUseScope::SanctionedResearch),
        Just(ProfileUseScope::Unknown),
    ]
}

fn ocr_backend_strategy() -> impl Strategy<Value = OcrBackend> {
    prop_oneof![
        Just(OcrBackend::Winrt),
        Just(OcrBackend::Crnn),
        Just(OcrBackend::Auto),
    ]
}

fn window_edge_strategy() -> impl Strategy<Value = WindowEdge> {
    prop_oneof![
        Just(WindowEdge::TopLeft),
        Just(WindowEdge::TopRight),
        Just(WindowEdge::BottomLeft),
        Just(WindowEdge::BottomRight),
    ]
}

fn profile_capture_target_strategy() -> impl Strategy<Value = ProfileCaptureTarget> {
    prop_oneof![
        Just(ProfileCaptureTarget::ForegroundWindow),
        Just(ProfileCaptureTarget::PrimaryMonitor),
        (0_u32..4).prop_map(|index| ProfileCaptureTarget::MonitorIndex { index }),
    ]
}

fn profile_capture_strategy() -> impl Strategy<Value = ProfileCapture> {
    (profile_capture_target_strategy(), 0_u32..250, any::<bool>()).prop_map(
        |(target, min_update_interval_ms, cursor_visible)| ProfileCapture {
            target,
            min_update_interval_ms,
            cursor_visible,
        },
    )
}

fn profile_match_strategy() -> impl Strategy<Value = ProfileMatch> {
    (
        prop::option::of(small_string()),
        prop::option::of(small_string()),
        prop::option::of(1_u32..1_000_000),
        prop::option::of(small_string()),
        prop::collection::vec(small_string(), 0..4),
    )
        .prop_map(
            |(exe, title_regex, steam_appid, window_class, process_args)| ProfileMatch {
                exe,
                title_regex,
                steam_appid,
                window_class,
                process_args,
            },
        )
}

fn profile_detection_strategy() -> impl Strategy<Value = ProfileDetection> {
    (
        prop::option::of(small_string()),
        prop::collection::vec(small_string(), 0..5),
        0.0_f32..1.0,
        0_u32..64,
    )
        .prop_map(
            |(model_id, classes_of_interest, confidence_threshold, max_detections)| {
                ProfileDetection {
                    model_id,
                    classes_of_interest,
                    confidence_threshold,
                    max_detections,
                }
            },
        )
}

fn hud_region_strategy() -> impl Strategy<Value = HudRegion> {
    prop_oneof![
        (-100_i32..100, -100_i32..100, 1_i32..400, 1_i32..400)
            .prop_map(|(x, y, w, h)| HudRegion::Absolute { x, y, w, h }),
        (0.0_f32..1.0, 0.0_f32..1.0, 0.01_f32..1.0, 0.01_f32..1.0)
            .prop_map(|(x, y, w, h)| HudRegion::FractionOfWindow { x, y, w, h }),
        (
            window_edge_strategy(),
            -500_i32..500,
            -500_i32..500,
            1_i32..400,
            1_i32..400,
        )
            .prop_map(
                |(edge, x_offset, y_offset, w, h)| HudRegion::AnchoredToEdge {
                    edge,
                    x_offset,
                    y_offset,
                    w,
                    h,
                }
            ),
    ]
}

fn hud_extractor_strategy() -> impl Strategy<Value = HudExtractor> {
    prop_oneof![
        Just(HudExtractor::WinrtOcr),
        small_string().prop_map(|model_id| HudExtractor::Crnn { model_id }),
        prop::collection::vec(small_string(), 0..4)
            .prop_map(|templates| HudExtractor::TemplateMatch { templates }),
        (
            prop::collection::vec((-50_i32..50, -50_i32..50), 0..4),
            small_string(),
        )
            .prop_map(|(sample_points, mapping)| HudExtractor::ColorRatio {
                sample_points,
                mapping,
            }),
    ]
}

fn hud_parser_strategy() -> impl Strategy<Value = HudParser> {
    prop_oneof![
        Just(HudParser::Number),
        Just(HudParser::FractionNumerator),
        Just(HudParser::FractionDenominator),
        (small_string(), 0_u32..5).prop_map(|(pattern, group)| HudParser::Regex { pattern, group }),
        prop::collection::btree_map(small_string(), small_string(), 0..4)
            .prop_map(|mapping| HudParser::Enum { mapping }),
    ]
}

fn profile_ocr_strategy() -> impl Strategy<Value = ProfileOcr> {
    (
        ocr_backend_strategy(),
        prop::collection::vec(hud_region_strategy(), 0..3),
        prop::collection::btree_map(small_string(), small_string(), 0..3),
    )
        .prop_map(|(default_backend, regions, parser_config)| ProfileOcr {
            default_backend,
            regions,
            parser_config,
        })
}

fn hud_field_spec_strategy() -> impl Strategy<Value = HudFieldSpec> {
    (
        small_string(),
        hud_region_strategy(),
        hud_extractor_strategy(),
        hud_parser_strategy(),
    )
        .prop_map(|(name, region, extractor, parser)| HudFieldSpec {
            name,
            region,
            extractor,
            parser,
        })
}

fn profile_backends_strategy() -> impl Strategy<Value = ProfileBackends> {
    (
        backend_strategy(),
        backend_strategy(),
        backend_strategy(),
        backend_strategy(),
    )
        .prop_map(
            |(default, keyboard_default, mouse_default, pad_default)| ProfileBackends {
                default,
                keyboard_default,
                mouse_default,
                pad_default,
            },
        )
}

fn event_filter_strategy() -> impl Strategy<Value = EventFilter> {
    prop_oneof![
        Just(EventFilter::All),
        Just(EventFilter::None),
        small_string().prop_map(|kind| EventFilter::Kind { kind }),
        Just(EventFilter::Source {
            source: EventSource::PerceptionHud,
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

fn event_extension_strategy() -> impl Strategy<Value = EventExtension> {
    (small_string(), event_filter_strategy(), small_string()).prop_map(
        |(name, from_filter, emits_kind)| EventExtension {
            name,
            from_filter,
            emits_kind,
        },
    )
}

fn profile_identity_strategy() -> impl Strategy<Value = (String, String, String)> {
    (small_string(), small_string()).prop_map(|(id, label)| (id, label, "1.0.0".to_owned()))
}

fn profile_strategy() -> impl Strategy<Value = Profile> {
    (
        profile_identity_strategy(),
        profile_use_scope_strategy(),
        prop::collection::vec(profile_match_strategy(), 0..3),
        perception_mode_strategy(),
        profile_capture_strategy(),
        profile_detection_strategy(),
        profile_ocr_strategy(),
        prop::collection::vec(hud_field_spec_strategy(), 0..3),
        prop::collection::btree_map(small_string(), small_string(), 0..4),
        profile_backends_strategy(),
        prop::collection::vec(event_extension_strategy(), 0..3),
    )
        .prop_map(
            |(
                (id, label, version),
                use_scope,
                matches,
                mode,
                capture,
                detection,
                ocr,
                hud,
                keymap,
                backends,
                event_extensions,
            )| Profile {
                id,
                label,
                version,
                use_scope,
                matches,
                mode,
                capture,
                detection,
                ocr,
                hud,
                keymap,
                backends,
                event_extensions,
            },
        )
}
