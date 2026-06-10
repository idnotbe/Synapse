use synapse_core::Rect;

use crate::{CaptureError, CapturedBgraBitmap, CapturedWindowBgraBitmap, platform};

// `CapturedFrame`/`CapturedSoftwareBitmap` only feed the Windows-only WinRT
// `SoftwareBitmap` helpers below.
#[cfg(windows)]
use crate::{CapturedFrame, CapturedSoftwareBitmap};

#[cfg(windows)]
/// Copies a captured frame region into a `WinRT` `SoftwareBitmap`.
///
/// # Errors
///
/// Returns [`CaptureError`] when the region is empty/outside the frame, the
/// frame format is unsupported, or the D3D/WinRT copy fails.
pub fn captured_frame_region_to_software_bitmap(
    frame: &CapturedFrame,
    region: Rect,
) -> Result<CapturedSoftwareBitmap, CaptureError> {
    platform::captured_frame_region_to_software_bitmap(frame, region)
}

#[cfg(windows)]
/// Copies a captured frame region into raw BGRA bytes.
///
/// # Errors
///
/// Returns [`CaptureError`] when the region is empty/outside the frame, the
/// frame format is unsupported, or the D3D copy fails.
pub fn captured_frame_region_to_bgra_bitmap(
    frame: &CapturedFrame,
    region: Rect,
) -> Result<CapturedBgraBitmap, CaptureError> {
    platform::captured_frame_region_to_bgra_bitmap(frame, region)
}

#[cfg(windows)]
/// Captures a screen-coordinate region into a `WinRT` `SoftwareBitmap`.
///
/// # Errors
///
/// Returns [`CaptureError`] when the region is empty or the `GDI`/`WinRT`
/// copy fails.
pub fn screen_region_to_software_bitmap(
    region: Rect,
) -> Result<CapturedSoftwareBitmap, CaptureError> {
    platform::screen_region_to_software_bitmap(region)
}

/// Captures a screen-coordinate region into raw BGRA bytes.
///
/// Available on all platforms so `synapse-mcp`'s OCR/detection callers compile
/// everywhere. The real GDI capture exists only on Windows; on non-Windows the
/// platform impl returns `Err(GraphicsApiUnsupported)` (it never fabricates
/// pixels), so callers fail loudly instead of acting on mock image data.
///
/// # Errors
///
/// Returns [`CaptureError`] when the region is empty or the `GDI` capture fails
/// (Windows), or `GraphicsApiUnsupported` on any non-Windows build.
pub fn screen_region_to_bgra_bitmap(region: Rect) -> Result<CapturedBgraBitmap, CaptureError> {
    platform::screen_region_to_bgra_bitmap(region)
}

/// Captures a window-relative region into raw BGRA bytes. Windows uses WGC
/// `CreateForWindow` first and falls back to `PrintWindow(PW_RENDERFULLCONTENT)`
/// with the backend reported in the result. Non-Windows builds fail loudly.
///
/// # Errors
///
/// Returns [`CaptureError`] when the HWND/region is invalid, no WGC frame
/// arrives, `PrintWindow` fails, or the bitmap copy fails.
pub fn window_region_to_bgra_bitmap(
    hwnd: i64,
    region: Rect,
    timeout_ms: u64,
) -> Result<CapturedWindowBgraBitmap, CaptureError> {
    platform::window_region_to_bgra_bitmap(hwnd, region, timeout_ms)
}

/// Converts a client-relative region to the full-window coordinate space used
/// by WGC/PrintWindow frames for this HWND.
///
/// # Errors
///
/// Returns [`CaptureError`] when the HWND cannot be resolved.
pub fn client_region_to_window_region(hwnd: i64, region: Rect) -> Result<Rect, CaptureError> {
    platform::client_region_to_window_region(hwnd, region)
}
