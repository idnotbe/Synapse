#[cfg(windows)]
use synapse_core::Rect;

#[cfg(windows)]
use crate::{CaptureError, CapturedBgraBitmap, CapturedFrame, CapturedSoftwareBitmap, platform};

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

#[cfg(windows)]
/// Captures a screen-coordinate region into raw BGRA bytes.
///
/// # Errors
///
/// Returns [`CaptureError`] when the region is empty or the `GDI` capture fails.
pub fn screen_region_to_bgra_bitmap(region: Rect) -> Result<CapturedBgraBitmap, CaptureError> {
    platform::screen_region_to_bgra_bitmap(region)
}
