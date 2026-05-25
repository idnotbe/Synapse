use std::sync::Once;

use enigo::{Button as EnigoButton, Direction, Enigo, Mouse};
use synapse_core::{AimCurve, AimStyle, AimTarget, ButtonAction, MouseButton, MouseTarget, Point};
use windows::Win32::{
    Foundation::{E_ACCESSDENIED, POINT as WinPoint},
    UI::{
        HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
            SetProcessDpiAwarenessContext, SetThreadDpiAwarenessContext,
        },
        Input::KeyboardAndMouse::{
            INPUT, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_MOVE,
            MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
        },
        WindowsAndMessaging::{
            GetPhysicalCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
            SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetPhysicalCursorPos,
        },
    },
};

use super::{
    input::{mouse_input, send_input_batch},
    utils::{enigo, enigo_error, sleep_ms},
};
use crate::backend::mouse_coordinates::{VirtualDesktop, normalize_absolute_mouse_point};
use crate::{ActionError, EmitState, sample_curve};

const WHEEL_DELTA: i32 = 120;
static DPI_AWARENESS: Once = Once::new();

pub(super) fn cursor_position() -> Result<Point, ActionError> {
    activate_thread_dpi_awareness();
    let mut point = WinPoint { x: 0, y: 0 };
    // SAFETY: `point` is a valid writable POINT for the duration of the call.
    unsafe { GetPhysicalCursorPos(&raw mut point) }.map_err(|err| {
        ActionError::BackendUnavailable {
            detail: format!("GetPhysicalCursorPos failed: {err}"),
        }
    })?;
    Ok(mcp_point_from_cursor_api(Point {
        x: point.x,
        y: point.y,
    }))
}

#[tracing::instrument(skip_all, fields(action_kind = "software_mouse_move"))]
pub(super) fn mouse_move(target: &MouseTarget) -> Result<(), ActionError> {
    let MouseTarget::Screen { point } = target else {
        return Err(ActionError::TargetInvalid {
            detail: "software backend requires a resolved screen point for mouse movement"
                .to_owned(),
        });
    };
    send_absolute_mouse_move(*point, "absolute mouse move")
}

#[tracing::instrument(skip_all, fields(action_kind = "software_mouse_move_relative"))]
pub(super) fn mouse_move_relative(dx: f32, dy: f32) -> Result<(), ActionError> {
    #[allow(clippy::cast_possible_truncation)]
    let rounded = (dx.round() as i32, dy.round() as i32);
    if rounded.0 == 0 && rounded.1 == 0 {
        return Ok(());
    }
    let current = cursor_position()?;
    send_absolute_mouse_move(
        relative_mouse_target(current, rounded),
        "relative mouse move",
    )
}

#[tracing::instrument(skip_all, fields(action_kind = "software_mouse_button"))]
pub(super) fn mouse_button(
    button: MouseButton,
    action: ButtonAction,
    hold_ms: u32,
    state: &mut EmitState,
) -> Result<(), ActionError> {
    let mut enigo = enigo()?;
    let enigo_button = enigo_button(button);
    match action {
        ButtonAction::Down => {
            state.apply_mouse_button(button, ButtonAction::Down);
            enigo
                .button(enigo_button, Direction::Press)
                .map_err(enigo_error("emit mouse button"))
        }
        ButtonAction::Up => {
            enigo
                .button(enigo_button, Direction::Release)
                .map_err(enigo_error("emit mouse button"))?;
            state.apply_mouse_button(button, ButtonAction::Up);
            Ok(())
        }
        ButtonAction::Press => {
            state.apply_mouse_button(button, ButtonAction::Down);
            enigo
                .button(enigo_button, Direction::Press)
                .map_err(enigo_error("emit mouse button"))?;
            let _interrupted = sleep_ms(hold_ms);
            enigo
                .button(enigo_button, Direction::Release)
                .map_err(enigo_error("emit mouse button"))?;
            state.apply_mouse_button(button, ButtonAction::Up);
            Ok(())
        }
    }
}

#[tracing::instrument(skip_all, fields(action_kind = "software_mouse_drag"))]
pub(super) fn mouse_drag(
    from: Point,
    to: Point,
    button: MouseButton,
    curve: &AimCurve,
    duration_ms: u32,
    state: &mut EmitState,
) -> Result<(), ActionError> {
    send_absolute_mouse_move(from, "drag origin absolute mouse move")?;
    mouse_button(button, ButtonAction::Down, 0, state)?;
    mouse_move_curve(from, to, curve, duration_ms)?;
    mouse_button(button, ButtonAction::Up, 0, state)
}

#[tracing::instrument(skip_all, fields(action_kind = "software_mouse_scroll"))]
pub(super) fn mouse_scroll(dy: i32, dx: i32, at: Option<Point>) -> Result<(), ActionError> {
    if let Some(point) = at {
        send_absolute_mouse_move(point, "scroll point absolute mouse move")?;
    }
    let mut inputs = Vec::with_capacity(2);
    if dy != 0 {
        inputs.push(mouse_input(
            0,
            0,
            signed_to_u32(dy.saturating_mul(WHEEL_DELTA)),
            MOUSEEVENTF_WHEEL,
        ));
    }
    if dx != 0 {
        inputs.push(mouse_input(
            0,
            0,
            signed_to_u32(dx.saturating_mul(WHEEL_DELTA)),
            MOUSEEVENTF_HWHEEL,
        ));
    }
    send_input_batch(&inputs, "mouse scroll")
}

#[tracing::instrument(skip_all, fields(action_kind = "software_aim_at"))]
pub(super) fn aim_at(target: &AimTarget, style: AimStyle) -> Result<(), ActionError> {
    if style == AimStyle::Track {
        return Err(ActionError::BackendUnavailable {
            detail: "track aim requires the M3 reflex runtime".to_owned(),
        });
    }
    let AimTarget::Screen { point } = target else {
        return Err(ActionError::TargetInvalid {
            detail: "software aim requires a resolved screen point".to_owned(),
        });
    };
    mouse_move(&MouseTarget::Screen { point: *point })
}

pub(super) fn release_buttons_with(
    enigo: &mut Enigo,
    buttons: &[MouseButton],
) -> Result<(), ActionError> {
    for button in buttons.iter().rev() {
        enigo
            .button(enigo_button(*button), Direction::Release)
            .map_err(enigo_error("release held mouse button"))?;
    }
    Ok(())
}

fn mouse_move_curve(
    from: Point,
    to: Point,
    curve: &AimCurve,
    duration_ms: u32,
) -> Result<(), ActionError> {
    let samples = sample_curve(curve, from, to, duration_ms, None);
    let desktop = virtual_desktop()?;
    let mut inputs = Vec::with_capacity(samples.len().saturating_sub(1));
    for point in samples.into_iter().skip(1) {
        inputs.push(absolute_mouse_input_for_desktop(point, desktop));
    }
    send_input_batch(&inputs, "drag curve absolute mouse move")
}

const fn enigo_button(button: MouseButton) -> EnigoButton {
    match button {
        MouseButton::Left => EnigoButton::Left,
        MouseButton::Right => EnigoButton::Right,
        MouseButton::Middle => EnigoButton::Middle,
        MouseButton::X1 => EnigoButton::Back,
        MouseButton::X2 => EnigoButton::Forward,
    }
}

fn send_absolute_mouse_move(point: Point, detail: &'static str) -> Result<(), ActionError> {
    activate_thread_dpi_awareness();
    // Physical cursor APIs avoid DPI virtualization drift between the MCP
    // process and the operator-visible screen coordinate space.
    let point = cursor_api_point_from_mcp(point);
    unsafe { SetPhysicalCursorPos(point.x, point.y) }.map_err(|error| {
        ActionError::BackendUnavailable {
            detail: format!("SetPhysicalCursorPos failed for {detail}: {error}"),
        }
    })
}

fn absolute_mouse_input_for_desktop(point: Point, desktop: VirtualDesktop) -> INPUT {
    let normalized = normalize_absolute_mouse_point(point, desktop);
    mouse_input(
        normalized.dx,
        normalized.dy,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )
}

const fn relative_mouse_target(current: Point, rounded: (i32, i32)) -> Point {
    Point {
        x: current.x.saturating_add(rounded.0),
        y: current.y.saturating_add(rounded.1),
    }
}

fn mcp_point_from_cursor_api(point: Point) -> Point {
    let scale = cursor_dpi_scale();
    Point {
        x: round_scaled(f64::from(point.x) / scale),
        y: round_scaled(f64::from(point.y) / scale),
    }
}

fn cursor_api_point_from_mcp(point: Point) -> Point {
    let scale = cursor_dpi_scale();
    Point {
        x: round_scaled(f64::from(point.x) * scale),
        y: round_scaled(f64::from(point.y) * scale),
    }
}

fn cursor_dpi_scale() -> f64 {
    let dpi = unsafe { GetDpiForSystem() };
    if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 }
}

#[allow(clippy::cast_possible_truncation)]
fn round_scaled(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn virtual_desktop() -> Result<VirtualDesktop, ActionError> {
    activate_thread_dpi_awareness();
    // SAFETY: GetSystemMetrics is read-only for these virtual-screen metrics.
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    VirtualDesktop::new(left, top, width, height).ok_or_else(|| ActionError::BackendUnavailable {
        detail: format!(
            "invalid virtual desktop metrics left={left} top={top} width={width} height={height}"
        ),
    })
}

const fn signed_to_u32(value: i32) -> u32 {
    u32::from_ne_bytes(value.to_ne_bytes())
}

fn ensure_dpi_awareness() {
    DPI_AWARENESS.call_once(|| {
        match unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } {
            Ok(()) => {}
            Err(error) if error.code() == E_ACCESSDENIED => {}
            Err(error) => {
                tracing::warn!(
                    component = "software_mouse",
                    error = %error,
                    "failed to set process DPI awareness; cursor coordinates may be virtualized"
                );
            }
        }
    });
}

fn activate_thread_dpi_awareness() {
    ensure_dpi_awareness();
    let _previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_mouse_target_uses_current_cursor_plus_delta() {
        let target = relative_mouse_target(Point { x: 10, y: 20 }, (7, -3));

        assert_eq!(target, Point { x: 17, y: 17 });
    }

    #[test]
    fn cursor_api_conversion_round_trips_system_dpi_coordinates() {
        let point = Point { x: 421, y: 426 };
        let api_point = cursor_api_point_from_mcp(point);
        let restored = mcp_point_from_cursor_api(api_point);

        assert_eq!(restored, point);
    }
}
