use std::{cell::RefCell, ffi::c_void, slice, time::Duration};

use synapse_core::Rect;
use windows::{
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Storage::Streams::DataWriter,
    Win32::Graphics::{
        Direct3D11::{
            D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
            D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, ID3D11Resource, ID3D11Texture2D,
        },
        Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetWindowDC, HBITMAP, HDC, HGDIOBJ,
            ReleaseDC, SRCCOPY, SelectObject,
        },
    },
    Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
    Win32::UI::WindowsAndMessaging::{GetWindowRect, PW_RENDERFULLCONTENT},
    core::Interface as _,
};

use crate::{
    CaptureBackendPreference, CaptureConfig, CaptureError, CaptureTarget, CapturedBgraBitmap,
    CapturedFrame, CapturedSoftwareBitmap, CapturedWindowBgraBitmap, DxgiFormat,
    spawn_capture_loop,
};

use super::common::{capture_unsupported, hwnd_from_i64};

thread_local! {
    static SCREEN_CAPTURE_SCRATCH: RefCell<Option<GdiCaptureScratch>> = const { RefCell::new(None) };
}

pub fn captured_frame_region_to_software_bitmap(
    frame: &CapturedFrame,
    region: Rect,
) -> Result<CapturedSoftwareBitmap, CaptureError> {
    let region = clamp_region_to_frame(frame, region)?;
    let bytes = copy_region_bgra(frame, region)?;
    let bitmap = software_bitmap_from_bgra(&bytes, region.w, region.h)?;
    Ok(CapturedSoftwareBitmap { region, bitmap })
}

pub fn captured_frame_region_to_bgra_bitmap(
    frame: &CapturedFrame,
    region: Rect,
) -> Result<CapturedBgraBitmap, CaptureError> {
    let region = clamp_region_to_frame(frame, region)?;
    let bytes = copy_region_bgra(frame, region)?;
    Ok(CapturedBgraBitmap {
        region,
        width: u32::try_from(region.w).unwrap_or_default(),
        height: u32::try_from(region.h).unwrap_or_default(),
        bytes,
    })
}

pub fn screen_region_to_software_bitmap(
    region: Rect,
) -> Result<CapturedSoftwareBitmap, CaptureError> {
    validate_bitmap_region(region)?;
    let bytes = copy_screen_region_bgra(region)?;
    let bitmap = software_bitmap_from_bgra(&bytes, region.w, region.h)?;
    Ok(CapturedSoftwareBitmap { region, bitmap })
}

pub fn screen_region_to_bgra_bitmap(region: Rect) -> Result<CapturedBgraBitmap, CaptureError> {
    validate_bitmap_region(region)?;
    let bytes = copy_screen_region_bgra(region)?;
    Ok(CapturedBgraBitmap {
        region,
        width: u32::try_from(region.w).unwrap_or_default(),
        height: u32::try_from(region.h).unwrap_or_default(),
        bytes,
    })
}

pub fn window_region_to_bgra_bitmap(
    hwnd: i64,
    region: Rect,
    timeout_ms: u64,
) -> Result<CapturedWindowBgraBitmap, CaptureError> {
    validate_bitmap_region(region)?;
    match graphics_capture_window_region_to_bgra_bitmap(hwnd, region, timeout_ms) {
        Ok(bitmap) if !is_all_zero_bgra(&bitmap.bytes) => Ok(CapturedWindowBgraBitmap {
            bitmap,
            capture_backend: "graphics_capture_window_bgra",
        }),
        Ok(_bitmap) => match printwindow_region_to_bgra_bitmap(hwnd, region) {
            Ok(printwindow_bitmap) if !is_all_zero_bgra(&printwindow_bitmap.bytes) => {
                tracing::warn!(
                    code = "CAPTURE_WGC_WINDOW_BLANK_PRINTWINDOW_USED",
                    hwnd,
                    region = ?region,
                    "WGC window capture returned all-zero pixels; PrintWindow produced non-zero readback"
                );
                Ok(CapturedWindowBgraBitmap {
                    bitmap: printwindow_bitmap,
                    capture_backend: "printwindow_render_full_content_bgra",
                })
            }
            Ok(_printwindow_bitmap) => {
                tracing::error!(
                    code = "CAPTURE_WINDOW_ALL_ZERO_FAILED",
                    hwnd,
                    region = ?region,
                    "WGC and PrintWindow both returned all-zero pixels; refusing to report a silent black frame"
                );
                Err(CaptureError::GraphicsApiUnsupported {
                    detail: format!(
                        "window capture for hwnd {hwnd:#x} region {region:?} produced all-zero pixels through both WGC and PrintWindow"
                    ),
                })
            }
            Err(printwindow_error) => {
                tracing::error!(
                    code = "CAPTURE_PRINTWINDOW_FALLBACK_AFTER_BLANK_FAILED",
                    hwnd,
                    region = ?region,
                    error = %printwindow_error,
                    "WGC window capture returned all-zero pixels and PrintWindow fallback failed"
                );
                Err(CaptureError::GraphicsApiUnsupported {
                    detail: format!(
                        "WGC window capture for hwnd {hwnd:#x} region {region:?} produced all-zero pixels and PrintWindow fallback failed: {printwindow_error}"
                    ),
                })
            }
        },
        Err(wgc_error) => {
            tracing::warn!(
                code = "CAPTURE_WGC_WINDOW_FAILED_PRINTWINDOW_FALLBACK",
                hwnd,
                region = ?region,
                error = %wgc_error,
                "WGC window capture failed; trying PrintWindow fallback"
            );
            printwindow_region_to_bgra_bitmap(hwnd, region)
                .map(|bitmap| CapturedWindowBgraBitmap {
                    bitmap,
                    capture_backend: "printwindow_render_full_content_bgra",
                })
                .map_err(|printwindow_error| CaptureError::GraphicsApiUnsupported {
                    detail: format!(
                        "window capture failed for hwnd {hwnd:#x}: WGC error: {wgc_error}; PrintWindow error: {printwindow_error}"
                    ),
                })
        }
    }
}

pub fn client_region_to_window_region(hwnd: i64, region: Rect) -> Result<Rect, CaptureError> {
    validate_bitmap_region(region)?;
    let hwnd = hwnd_from_i64(hwnd);
    let mut window_rect = windows::Win32::Foundation::RECT::default();
    unsafe { GetWindowRect(hwnd, &raw mut window_rect) }.map_err(capture_unsupported)?;
    let mut client_origin = windows::Win32::Foundation::POINT { x: 0, y: 0 };
    if !unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &raw mut client_origin) }
        .as_bool()
    {
        return Err(CaptureError::TargetInvalid {
            detail: "ClientToScreen failed while converting screenshot region".to_owned(),
        });
    }
    let offset_x = client_origin.x.saturating_sub(window_rect.left);
    let offset_y = client_origin.y.saturating_sub(window_rect.top);
    Ok(Rect {
        x: region.x.saturating_add(offset_x),
        y: region.y.saturating_add(offset_y),
        w: region.w,
        h: region.h,
    })
}

fn graphics_capture_window_region_to_bgra_bitmap(
    hwnd: i64,
    region: Rect,
    timeout_ms: u64,
) -> Result<CapturedBgraBitmap, CaptureError> {
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let handle = spawn_capture_loop(CaptureConfig {
        target: CaptureTarget::Window { hwnd },
        min_update_interval_ms: 16,
        cursor_visible: false,
        secondary_windows: false,
        dirty_region_only: false,
        backend_preference: CaptureBackendPreference::GraphicsCaptureApi,
    })?;
    let receiver = handle.receiver();
    let frame = match receiver.recv_timeout(timeout) {
        Ok(frame) => frame,
        Err(error) => {
            let stop_result = handle.stop();
            return Err(match stop_result {
                Ok(()) => CaptureError::ThreadFailed {
                    detail: format!(
                        "timed out after {timeout_ms} ms waiting for WGC window frame: {error}"
                    ),
                },
                Err(stop_error) => stop_error,
            });
        }
    };
    let result = captured_frame_region_to_bgra_bitmap(&frame, region);
    let stop_result = handle.stop();
    match (result, stop_result) {
        (Ok(bitmap), Ok(())) => Ok(bitmap),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) | (Err(_), Err(error)) => Err(error),
    }
}

fn printwindow_region_to_bgra_bitmap(
    hwnd: i64,
    region: Rect,
) -> Result<CapturedBgraBitmap, CaptureError> {
    let hwnd = hwnd_from_i64(hwnd);
    let mut window_rect = windows::Win32::Foundation::RECT::default();
    unsafe { GetWindowRect(hwnd, &raw mut window_rect) }.map_err(capture_unsupported)?;
    let window_width = window_rect.right.saturating_sub(window_rect.left);
    let window_height = window_rect.bottom.saturating_sub(window_rect.top);
    validate_region_inside_window(region, window_width, window_height)?;

    let window_dc = unsafe { GetWindowDC(Some(hwnd)) };
    if window_dc.is_invalid() {
        return Err(CaptureError::GraphicsApiUnsupported {
            detail: "GetWindowDC returned null for PrintWindow capture".to_owned(),
        });
    }
    let memory_dc = unsafe { CreateCompatibleDC(Some(window_dc)) };
    if memory_dc.is_invalid() {
        let _ = unsafe { ReleaseDC(Some(hwnd), window_dc) };
        return Err(CaptureError::GraphicsApiUnsupported {
            detail: "CreateCompatibleDC returned null for PrintWindow capture".to_owned(),
        });
    }
    let full_width = u32::try_from(window_width).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let full_height = u32::try_from(window_height).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let full_byte_len = bitmap_byte_len(full_width, full_height, "PrintWindow full window")?;
    let (bitmap, bits) = create_top_down_dib(window_dc, full_width, full_height, full_byte_len)?;
    let old_object = unsafe { SelectObject(memory_dc, HGDIOBJ::from(bitmap)) };
    if old_object.is_invalid() {
        let _ = unsafe { DeleteObject(HGDIOBJ::from(bitmap)) };
        let _ = unsafe { DeleteDC(memory_dc) };
        let _ = unsafe { ReleaseDC(Some(hwnd), window_dc) };
        return Err(CaptureError::GraphicsApiUnsupported {
            detail: "SelectObject failed for PrintWindow bitmap".to_owned(),
        });
    }
    let result = unsafe { PrintWindow(hwnd, memory_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)) };
    let output = if result.as_bool() {
        copy_bgra_region_from_top_down_dib(bits, full_width, full_height, region)
    } else {
        Err(CaptureError::GraphicsApiUnsupported {
            detail: "PrintWindow returned false".to_owned(),
        })
    };
    let _ = unsafe { SelectObject(memory_dc, old_object) };
    let _ = unsafe { DeleteObject(HGDIOBJ::from(bitmap)) };
    let _ = unsafe { DeleteDC(memory_dc) };
    let _ = unsafe { ReleaseDC(Some(hwnd), window_dc) };
    let bytes = output?;
    Ok(CapturedBgraBitmap {
        region,
        width: u32::try_from(region.w).unwrap_or_default(),
        height: u32::try_from(region.h).unwrap_or_default(),
        bytes,
    })
}

fn software_bitmap_from_bgra(
    bytes: &[u8],
    width: i32,
    height: i32,
) -> Result<SoftwareBitmap, CaptureError> {
    let writer = DataWriter::new().map_err(capture_unsupported)?;
    writer.WriteBytes(bytes).map_err(capture_unsupported)?;
    let buffer = writer.DetachBuffer().map_err(capture_unsupported)?;
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        width,
        height,
        BitmapAlphaMode::Premultiplied,
    )
    .map_err(capture_unsupported)
}
fn copy_region_bgra(frame: &CapturedFrame, region: Rect) -> Result<Vec<u8>, CaptureError> {
    let convert_rgba_to_bgra = match frame.format {
        DxgiFormat::Bgra8 | DxgiFormat::Bgra8Srgb => false,
        DxgiFormat::Rgba8 | DxgiFormat::Rgba8Srgb => true,
        other => {
            return Err(CaptureError::GraphicsApiUnsupported {
                detail: format!("OCR bitmap copy does not support frame format {other:?}"),
            });
        }
    };

    let width = u32::try_from(region.w).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let height = u32::try_from(region.h).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let texture = frame.texture.get();
    let staging = create_staging_texture(texture, width, height)?;
    let context = unsafe { texture.GetDevice() }
        .and_then(|device| unsafe { device.GetImmediateContext() })
        .map_err(capture_unsupported)?;
    let source: ID3D11Resource = texture.cast().map_err(capture_unsupported)?;
    let target: ID3D11Resource = staging.cast().map_err(capture_unsupported)?;
    let source_box = D3D11_BOX {
        left: u32::try_from(region.x).unwrap_or(0),
        top: u32::try_from(region.y).unwrap_or(0),
        front: 0,
        right: u32::try_from(region.x.saturating_add(region.w)).unwrap_or(width),
        bottom: u32::try_from(region.y.saturating_add(region.h)).unwrap_or(height),
        back: 1,
    };
    unsafe {
        context.CopySubresourceRegion(&target, 0, 0, 0, 0, &source, 0, Some(&raw const source_box));
    }

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&target, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped)) }
        .map_err(capture_unsupported)?;
    let bytes = copy_mapped_rows(&mapped, width, height, convert_rgba_to_bgra);
    unsafe {
        context.Unmap(&target, 0);
    }
    bytes
}

fn create_staging_texture(
    texture: &ID3D11Texture2D,
    width: u32,
    height: u32,
) -> Result<ID3D11Texture2D, CaptureError> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        texture.GetDesc(&raw mut desc);
    }
    desc.Width = width;
    desc.Height = height;
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0.cast_unsigned();
    desc.MiscFlags = 0;
    desc.SampleDesc.Count = 1;
    desc.SampleDesc.Quality = 0;

    let device = unsafe { texture.GetDevice() }.map_err(capture_unsupported)?;
    let mut staging = None;
    unsafe { device.CreateTexture2D(&raw const desc, None, Some(&raw mut staging)) }
        .map_err(capture_unsupported)?;
    staging.ok_or_else(|| CaptureError::GraphicsApiUnsupported {
        detail: "CreateTexture2D returned no staging texture".to_owned(),
    })
}

fn copy_mapped_rows(
    mapped: &D3D11_MAPPED_SUBRESOURCE,
    width: u32,
    height: u32,
    convert_rgba_to_bgra: bool,
) -> Result<Vec<u8>, CaptureError> {
    let row_len = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| CaptureError::TargetInvalid {
            detail: format!("invalid OCR bitmap width {width}"),
        })?;
    let height = usize::try_from(height).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let row_pitch =
        usize::try_from(mapped.RowPitch).map_err(|err| CaptureError::GraphicsApiUnsupported {
            detail: err.to_string(),
        })?;
    let mut output = vec![0_u8; row_len.saturating_mul(height)];
    for row in 0..height {
        let source = unsafe {
            slice::from_raw_parts((mapped.pData as *const u8).add(row * row_pitch), row_len)
        };
        let start = row.saturating_mul(row_len);
        output[start..start + row_len].copy_from_slice(source);
    }
    if convert_rgba_to_bgra {
        for pixel in output.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    Ok(output)
}

fn copy_screen_region_bgra(region: Rect) -> Result<Vec<u8>, CaptureError> {
    let width = u32::try_from(region.w).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let height = u32::try_from(region.h).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let byte_len = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| CaptureError::TargetInvalid {
            detail: format!("invalid screen capture region {region:?}"),
        })?;
    let screen_dc = unsafe { GetDC(None) };
    if screen_dc.is_invalid() {
        return Err(CaptureError::GraphicsApiUnsupported {
            detail: "GetDC returned null".to_owned(),
        });
    }
    let memory_dc = unsafe { CreateCompatibleDC(Some(screen_dc)) };
    if memory_dc.is_invalid() {
        let _ = unsafe { ReleaseDC(None, screen_dc) };
        return Err(CaptureError::GraphicsApiUnsupported {
            detail: "CreateCompatibleDC returned null".to_owned(),
        });
    }
    let result = SCREEN_CAPTURE_SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        let needs_recreate = scratch
            .as_ref()
            .is_none_or(|scratch| !scratch.matches(width, height, byte_len));
        if needs_recreate {
            *scratch = Some(GdiCaptureScratch::new(
                screen_dc, memory_dc, width, height, byte_len,
            )?);
        } else {
            let _ = unsafe { DeleteDC(memory_dc) };
        }
        let scratch = scratch
            .as_ref()
            .ok_or_else(|| CaptureError::GraphicsApiUnsupported {
                detail: "screen capture scratch buffer was not initialized".to_owned(),
            })?;
        let bitblt = unsafe {
            BitBlt(
                scratch.memory_dc,
                0,
                0,
                region.w,
                region.h,
                Some(screen_dc),
                region.x,
                region.y,
                SRCCOPY,
            )
        };
        bitblt.map_err(capture_unsupported)?;
        Ok(unsafe { slice::from_raw_parts(scratch.bits.cast::<u8>(), byte_len) }.to_vec())
    });
    let _ = unsafe { ReleaseDC(None, screen_dc) };
    result
}

fn bitmap_byte_len(width: u32, height: u32, label: &str) -> Result<usize, CaptureError> {
    usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| CaptureError::TargetInvalid {
            detail: format!("invalid {label} bitmap size {width}x{height}"),
        })
}

fn create_top_down_dib(
    reference_dc: HDC,
    width: u32,
    height: u32,
    byte_len: usize,
) -> Result<(HBITMAP, *mut c_void), CaptureError> {
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(u32::MAX),
            biWidth: i32::try_from(width).unwrap_or(i32::MAX),
            biHeight: -i32::try_from(height).unwrap_or(i32::MAX),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: u32::try_from(byte_len).unwrap_or(u32::MAX),
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut bits = std::ptr::null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            Some(reference_dc),
            &raw const bitmap_info,
            DIB_RGB_COLORS,
            &raw mut bits,
            None,
            0,
        )
    }
    .map_err(capture_unsupported)?;
    if bits.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ::from(bitmap)) };
        return Err(CaptureError::GraphicsApiUnsupported {
            detail: "CreateDIBSection returned no bitmap bits".to_owned(),
        });
    }
    Ok((bitmap, bits))
}

fn copy_bgra_region_from_top_down_dib(
    bits: *const c_void,
    full_width: u32,
    full_height: u32,
    region: Rect,
) -> Result<Vec<u8>, CaptureError> {
    let region_width = u32::try_from(region.w).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let region_height = u32::try_from(region.h).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    validate_region_inside_window(
        region,
        i32::try_from(full_width).unwrap_or(i32::MAX),
        i32::try_from(full_height).unwrap_or(i32::MAX),
    )?;
    let output_row_len = usize::try_from(region_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| CaptureError::TargetInvalid {
            detail: format!("invalid output bitmap width {region_width}"),
        })?;
    let source_row_len = usize::try_from(full_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| CaptureError::TargetInvalid {
            detail: format!("invalid source bitmap width {full_width}"),
        })?;
    let output_len = bitmap_byte_len(region_width, region_height, "PrintWindow crop")?;
    let source_len = bitmap_byte_len(full_width, full_height, "PrintWindow source")?;
    let source = unsafe { slice::from_raw_parts(bits.cast::<u8>(), source_len) };
    let mut output = vec![0_u8; output_len];
    let start_x = usize::try_from(region.x).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let start_y = usize::try_from(region.y).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let height = usize::try_from(region_height).map_err(|err| CaptureError::TargetInvalid {
        detail: err.to_string(),
    })?;
    let source_x_offset = start_x
        .checked_mul(4)
        .ok_or_else(|| CaptureError::TargetInvalid {
            detail: format!("invalid source x offset {}", region.x),
        })?;
    for row in 0..height {
        let source_start = start_y
            .checked_add(row)
            .and_then(|source_row| source_row.checked_mul(source_row_len))
            .and_then(|row_start| row_start.checked_add(source_x_offset))
            .ok_or_else(|| CaptureError::TargetInvalid {
                detail: format!("invalid PrintWindow crop row {row} for region {region:?}"),
            })?;
        let output_start =
            row.checked_mul(output_row_len)
                .ok_or_else(|| CaptureError::TargetInvalid {
                    detail: format!("invalid output row {row} for region {region:?}"),
                })?;
        output[output_start..output_start + output_row_len]
            .copy_from_slice(&source[source_start..source_start + output_row_len]);
    }
    Ok(output)
}

fn validate_region_inside_window(
    region: Rect,
    window_width: i32,
    window_height: i32,
) -> Result<(), CaptureError> {
    validate_bitmap_region(region)?;
    if region.x < 0
        || region.y < 0
        || region.x.saturating_add(region.w) > window_width
        || region.y.saturating_add(region.h) > window_height
    {
        return Err(CaptureError::TargetInvalid {
            detail: format!(
                "window capture region {region:?} is outside window bitmap bounds {window_width}x{window_height}"
            ),
        });
    }
    Ok(())
}

fn is_all_zero_bgra(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

struct GdiCaptureScratch {
    width: u32,
    height: u32,
    byte_len: usize,
    memory_dc: HDC,
    bitmap: HBITMAP,
    old_object: HGDIOBJ,
    bits: *mut c_void,
}

impl GdiCaptureScratch {
    fn new(
        screen_dc: HDC,
        memory_dc: HDC,
        width: u32,
        height: u32,
        byte_len: usize,
    ) -> Result<Self, CaptureError> {
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(u32::MAX),
                biWidth: i32::try_from(width).unwrap_or(i32::MAX),
                biHeight: -i32::try_from(height).unwrap_or(i32::MAX),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: u32::try_from(byte_len).unwrap_or(u32::MAX),
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(
                Some(screen_dc),
                &raw const bitmap_info,
                DIB_RGB_COLORS,
                &raw mut bits,
                None,
                0,
            )
        }
        .map_err(capture_unsupported)?;
        if bits.is_null() {
            let _ = unsafe { DeleteObject(HGDIOBJ::from(bitmap)) };
            let _ = unsafe { DeleteDC(memory_dc) };
            return Err(CaptureError::GraphicsApiUnsupported {
                detail: "CreateDIBSection returned no bitmap bits".to_owned(),
            });
        }
        let old_object = unsafe { SelectObject(memory_dc, HGDIOBJ::from(bitmap)) };
        if old_object.is_invalid() {
            let _ = unsafe { DeleteObject(HGDIOBJ::from(bitmap)) };
            let _ = unsafe { DeleteDC(memory_dc) };
            return Err(CaptureError::GraphicsApiUnsupported {
                detail: "SelectObject failed for screen capture bitmap".to_owned(),
            });
        }
        Ok(Self {
            width,
            height,
            byte_len,
            memory_dc,
            bitmap,
            old_object,
            bits,
        })
    }

    const fn matches(&self, width: u32, height: u32, byte_len: usize) -> bool {
        self.width == width && self.height == height && self.byte_len == byte_len
    }
}

impl Drop for GdiCaptureScratch {
    fn drop(&mut self) {
        let _ = unsafe { SelectObject(self.memory_dc, self.old_object) };
        let _ = unsafe { DeleteObject(HGDIOBJ::from(self.bitmap)) };
        let _ = unsafe { DeleteDC(self.memory_dc) };
    }
}

fn validate_bitmap_region(region: Rect) -> Result<(), CaptureError> {
    if region.w <= 0 || region.h <= 0 {
        return Err(CaptureError::TargetInvalid {
            detail: format!("empty bitmap capture region {region:?}"),
        });
    }
    Ok(())
}

fn clamp_region_to_frame(frame: &CapturedFrame, region: Rect) -> Result<Rect, CaptureError> {
    if region.w <= 0 || region.h <= 0 {
        return Err(CaptureError::TargetInvalid {
            detail: format!("empty OCR capture region {region:?}"),
        });
    }
    let frame_w = i64::from(frame.width);
    let frame_h = i64::from(frame.height);
    let left = i64::from(region.x).clamp(0, frame_w);
    let top = i64::from(region.y).clamp(0, frame_h);
    let right = i64::from(region.x)
        .saturating_add(i64::from(region.w))
        .clamp(0, frame_w);
    let bottom = i64::from(region.y)
        .saturating_add(i64::from(region.h))
        .clamp(0, frame_h);
    if right <= left || bottom <= top {
        return Err(CaptureError::TargetInvalid {
            detail: format!("OCR capture region {region:?} is outside frame bounds"),
        });
    }
    Ok(Rect {
        x: i32::try_from(left).unwrap_or(i32::MAX),
        y: i32::try_from(top).unwrap_or(i32::MAX),
        w: i32::try_from(right - left).unwrap_or(i32::MAX),
        h: i32::try_from(bottom - top).unwrap_or(i32::MAX),
    })
}
