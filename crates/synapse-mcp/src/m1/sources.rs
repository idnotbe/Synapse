use std::collections::BTreeMap;

use chrono::Utc;
use rmcp::ErrorData;
use synapse_core::{
    AccessibleNode, AudioContext, DetectedEntity, FocusedElement, ForegroundContext, HudReadings,
    PerceptionMode, Rect, SensorStatus, UiaPattern, element_id, entity_id,
};
use synapse_perception::ObservationInput;

pub fn synthetic_notepad_input() -> ObservationInput {
    let at = Utc::now();
    let focused_id = element_id(0x1234, "0000002a00000001");
    let elements = vec![
        node(0, 0, "Notepad", "Window", false),
        node(1, 1, "Document", "Edit", true),
        node(2, 1, "File", "MenuItem", false),
        node(3, 1, "Edit", "MenuItem", false),
        node(4, 1, "View", "MenuItem", false),
        node(5, 1, "Status", "Text", false),
    ];
    let mut latency = BTreeMap::new();
    latency.insert("a11y".to_owned(), 1.25);
    latency.insert("capture".to_owned(), 0.50);
    ObservationInput {
        foreground: ForegroundContext {
            hwnd: 0x1234,
            pid: 44,
            process_name: "notepad.exe".to_owned(),
            process_path: "C:\\Windows\\System32\\notepad.exe".to_owned(),
            window_title: "manual.txt - Notepad".to_owned(),
            window_bounds: Rect {
                x: 10,
                y: 20,
                w: 800,
                h: 600,
            },
            monitor_index: 0,
            dpi_scale: 1.0,
            profile_id: None,
            steam_appid: None,
            is_fullscreen: false,
            is_dwm_composed: true,
        },
        focused: Some(FocusedElement {
            element_id: focused_id,
            name: "Document".to_owned(),
            role: "Edit".to_owned(),
            automation_id: Some("15".to_owned()),
            bbox: Rect {
                x: 12,
                y: 80,
                w: 760,
                h: 480,
            },
            enabled: true,
            patterns: vec![UiaPattern::Text, UiaPattern::Value],
            value: Some("Synthetic Synapse text".to_owned()),
            selected_text: None,
        }),
        elements,
        entities: vec![DetectedEntity {
            entity_id: entity_id(9),
            track_id: 9,
            class_label: "cursor".to_owned(),
            bbox: Rect {
                x: 40,
                y: 90,
                w: 8,
                h: 20,
            },
            confidence: 0.80,
            first_seen_at: at,
            last_seen_at: at,
            velocity_px_per_s: None,
        }],
        hud: HudReadings::default(),
        audio: AudioContext::default(),
        recent_events: Vec::new(),
        clipboard_summary: None,
        fs_recent: Vec::new(),
        sensor_latency_ms: latency,
        a11y_status: SensorStatus::Healthy,
        capture_status: SensorStatus::Healthy,
        detection_status: SensorStatus::Disabled,
        audio_status: SensorStatus::Disabled,
        mode_override: None,
    }
}

fn node(sequence: u32, depth: u32, name: &str, role: &str, focused: bool) -> AccessibleNode {
    let depth_i32 = i32::try_from(depth).unwrap_or(0);
    let sequence_i32 = i32::try_from(sequence).unwrap_or(0);
    AccessibleNode {
        element_id: element_id(0x1234, &format!("0000002a{sequence:08x}")),
        parent: (depth > 0).then(|| element_id(0x1234, "0000002a00000000")),
        name: name.to_owned(),
        role: role.to_owned(),
        automation_id: None,
        bbox: Rect {
            x: 10 + depth_i32,
            y: 20 + sequence_i32.saturating_mul(10),
            w: 100,
            h: 30,
        },
        enabled: true,
        focused,
        patterns: Vec::new(),
        children_count: 0,
        depth,
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn platform_input(_depth: u32, mode: PerceptionMode) -> Result<ObservationInput, ErrorData> {
    linux_x11::platform_input(mode)
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
pub fn platform_input(_depth: u32, _mode: PerceptionMode) -> Result<ObservationInput, ErrorData> {
    Err(crate::m1::mcp_error(
        synapse_core::error_codes::OBSERVE_NO_PERCEPTION_AVAILABLE,
        "UIA foreground window lookup requires Windows",
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux_x11 {
    use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};

    use chrono::Utc;
    use rmcp::ErrorData;
    use synapse_core::{
        AccessibleNode, DetectedEntity, ForegroundContext, PerceptionMode, Rect, SensorStatus,
        element_id, entity_id, error_codes,
    };
    use synapse_perception::ObservationInput;
    use x11rb::{
        connection::Connection,
        protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, Window},
        rust_connection::RustConnection,
    };

    use crate::m1::mcp_error;

    pub fn platform_input(mode: PerceptionMode) -> Result<ObservationInput, ErrorData> {
        let started = Instant::now();
        let (conn, screen_num) = RustConnection::connect(None)
            .map_err(|err| unavailable(format!("X11 connect failed: {err}")))?;
        let screen =
            conn.setup().roots.get(screen_num).ok_or_else(|| {
                unavailable(format!("X11 screen index {screen_num} was not found"))
            })?;
        let root_bounds = Rect {
            x: 0,
            y: 0,
            w: i32::from(screen.width_in_pixels),
            h: i32::from(screen.height_in_pixels),
        };
        let active = active_window(&conn, screen.root);
        let window = active.unwrap_or(screen.root);
        let bounds = window_bounds(&conn, window).unwrap_or(root_bounds);
        let pid = window_pid(&conn, window).unwrap_or_default();
        let title = window_title(&conn, window)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| {
                if active.is_some() {
                    format!("X11 window 0x{window:x}")
                } else {
                    format!(
                        "X11 display {}",
                        std::env::var("DISPLAY").unwrap_or_default()
                    )
                }
            });
        let (process_name, process_path) = process_metadata(pid);
        let foreground = ForegroundContext {
            hwnd: i64::from(window),
            pid,
            process_name,
            process_path,
            window_title: title,
            window_bounds: bounds,
            monitor_index: u32::try_from(screen_num).unwrap_or(u32::MAX),
            dpi_scale: 1.0,
            profile_id: None,
            steam_appid: None,
            is_fullscreen: bounds.x <= root_bounds.x
                && bounds.y <= root_bounds.y
                && bounds.w >= root_bounds.w
                && bounds.h >= root_bounds.h,
            is_dwm_composed: false,
        };
        let mut input = ObservationInput::new(foreground.clone());
        input.elements = vec![window_node(&foreground)];
        input.focused = None;
        input.entities = cursor_entity().into_iter().collect();
        input.a11y_status = SensorStatus::DegradedSensorFailed {
            reason_code: "LINUX_X11_WINDOW_METADATA_ONLY".to_owned(),
        };
        input.capture_status = SensorStatus::Healthy;
        input.detection_status = SensorStatus::Disabled;
        input.audio_status = SensorStatus::Disabled;
        input.mode_override = Some(mode);
        input.sensor_latency_ms = BTreeMap::from([(
            "x11_window_metadata".to_owned(),
            started.elapsed().as_secs_f32() * 1000.0,
        )]);
        Ok(input)
    }

    fn active_window(conn: &RustConnection, root: Window) -> Option<Window> {
        let atom = intern_atom(conn, b"_NET_ACTIVE_WINDOW").ok()?;
        let reply = conn
            .get_property(false, root, atom, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let window = reply.value32()?.next()?;
        if window == 0 || window_bounds(conn, window).is_none() {
            None
        } else {
            Some(window)
        }
    }

    fn window_bounds(conn: &RustConnection, window: Window) -> Option<Rect> {
        let geometry = conn.get_geometry(window).ok()?.reply().ok()?;
        Some(Rect {
            x: i32::from(geometry.x),
            y: i32::from(geometry.y),
            w: i32::from(geometry.width),
            h: i32::from(geometry.height),
        })
    }

    fn window_title(conn: &RustConnection, window: Window) -> Option<String> {
        let utf8 = intern_atom(conn, b"UTF8_STRING").ok()?;
        let net_wm_name = intern_atom(conn, b"_NET_WM_NAME").ok()?;
        read_string_property(conn, window, net_wm_name, utf8).or_else(|| {
            read_string_property(
                conn,
                window,
                AtomEnum::WM_NAME.into(),
                AtomEnum::STRING.into(),
            )
        })
    }

    fn window_pid(conn: &RustConnection, window: Window) -> Option<u32> {
        let atom = intern_atom(conn, b"_NET_WM_PID").ok()?;
        let reply = conn
            .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        reply.value32()?.next()
    }

    fn read_string_property(
        conn: &RustConnection,
        window: Window,
        property: Atom,
        property_type: Atom,
    ) -> Option<String> {
        let reply = conn
            .get_property(false, window, property, property_type, 0, 4096)
            .ok()?
            .reply()
            .ok()?;
        let bytes = trim_nul(reply.value);
        if bytes.is_empty() {
            return None;
        }
        String::from_utf8(bytes).ok()
    }

    fn intern_atom(conn: &RustConnection, name: &[u8]) -> Result<Atom, ErrorData> {
        conn.intern_atom(false, name)
            .map_err(|err| unavailable(format!("X11 intern_atom failed: {err}")))?
            .reply()
            .map(|reply| reply.atom)
            .map_err(|err| unavailable(format!("X11 intern_atom reply failed: {err}")))
    }

    fn trim_nul(mut bytes: Vec<u8>) -> Vec<u8> {
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        bytes
    }

    fn window_node(foreground: &ForegroundContext) -> AccessibleNode {
        AccessibleNode {
            element_id: element_id(foreground.hwnd, "0000000000000000"),
            parent: None,
            name: foreground.window_title.clone(),
            role: "Window".to_owned(),
            automation_id: None,
            bbox: foreground.window_bounds,
            enabled: true,
            focused: false,
            patterns: Vec::new(),
            children_count: 0,
            depth: 0,
        }
    }

    fn cursor_entity() -> Option<DetectedEntity> {
        let point = synapse_action::backend::software::cursor_position().ok()?;
        let at = Utc::now();
        Some(DetectedEntity {
            entity_id: entity_id(0),
            track_id: 0,
            class_label: "cursor".to_owned(),
            bbox: Rect {
                x: point.x,
                y: point.y,
                w: 1,
                h: 1,
            },
            confidence: 1.0,
            first_seen_at: at,
            last_seen_at: at,
            velocity_px_per_s: None,
        })
    }

    fn process_metadata(pid: u32) -> (String, String) {
        if pid == 0 {
            return (
                "x11".to_owned(),
                std::env::var("DISPLAY").unwrap_or_default(),
            );
        }
        let comm_path = PathBuf::from(format!("/proc/{pid}/comm"));
        let name = fs::read_to_string(comm_path)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("pid:{pid}"));
        let process_path = fs::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        (name, process_path)
    }

    fn unavailable(detail: String) -> ErrorData {
        mcp_error(error_codes::OBSERVE_NO_PERCEPTION_AVAILABLE, detail)
    }
}

#[cfg(windows)]
pub fn platform_input(depth: u32, mode: PerceptionMode) -> Result<ObservationInput, ErrorData> {
    let root = synapse_a11y::focused_window().map_err(|err| a11y_error(&err))?;
    let tree = synapse_a11y::snapshot(&root, depth).map_err(|err| a11y_error(&err))?;
    let hwnd = tree
        .root
        .parts()
        .map_err(|err| {
            crate::m1::mcp_error(synapse_core::error_codes::OBSERVE_INTERNAL, err.to_string())
        })?
        .hwnd;
    let foreground = windows_foreground_context(hwnd)?;
    let focused = tree
        .nodes
        .iter()
        .find(|node| node.focused)
        .or_else(|| tree.nodes.first())
        .map(focused_from_node);
    let mut input = ObservationInput::new(foreground);
    input.focused = focused;
    input.elements = tree.nodes;
    input.a11y_status = SensorStatus::Healthy;
    input.capture_status = SensorStatus::Unavailable;
    if mode != PerceptionMode::Auto {
        input.mode_override = Some(mode);
    }
    Ok(input)
}

#[cfg(windows)]
fn a11y_error(err: &synapse_a11y::A11yError) -> ErrorData {
    match err {
        synapse_a11y::A11yError::NoForeground { .. }
        | synapse_a11y::A11yError::NotAvailable { .. } => crate::m1::mcp_error(
            synapse_core::error_codes::OBSERVE_NO_PERCEPTION_AVAILABLE,
            err.to_string(),
        ),
        _ => crate::m1::mcp_error(synapse_core::error_codes::OBSERVE_INTERNAL, err.to_string()),
    }
}

#[cfg(windows)]
fn focused_from_node(node: &AccessibleNode) -> FocusedElement {
    FocusedElement {
        element_id: node.element_id.clone(),
        name: node.name.clone(),
        role: node.role.clone(),
        automation_id: node.automation_id.clone(),
        bbox: node.bbox,
        enabled: node.enabled,
        patterns: node.patterns.clone(),
        value: None,
        selected_text: None,
    }
}

#[cfg(windows)]
fn windows_foreground_context(hwnd: i64) -> Result<ForegroundContext, ErrorData> {
    synapse_a11y::foreground_context(hwnd).map_err(|err| a11y_error(&err))
}
