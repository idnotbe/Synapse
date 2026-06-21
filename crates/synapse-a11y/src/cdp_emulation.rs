//! Target-scoped raw-CDP browser emulation helpers (#1173).

use serde::{Deserialize, Serialize};

use crate::{A11yError, A11yResult};

pub const CDP_DEVICE_METRICS_MAX_DIMENSION: u32 = 10_000_000;
pub const CDP_DEVICE_SCALE_FACTOR_MAX: f64 = 1000.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CdpViewportOverride {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: f64,
    pub mobile: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CdpViewportReadback {
    pub inner_width: i64,
    pub inner_height: i64,
    pub device_pixel_ratio: f64,
    pub screen_width: i64,
    pub screen_height: i64,
    pub outer_width: i64,
    pub outer_height: i64,
    pub visual_viewport_width: Option<f64>,
    pub visual_viewport_height: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CdpViewportResult {
    pub endpoint: String,
    pub cdp_target_id: String,
    pub operation: String,
    pub requested: Option<CdpViewportOverride>,
    pub page_url: String,
    pub page_title: String,
    pub ready_state: String,
    pub readback: CdpViewportReadback,
}

enum DeviceMetricsCommand {
    Set(CdpViewportOverride),
    Reset,
}

/// Applies `Emulation.setDeviceMetricsOverride` to one CDP page target, then
/// reads back page-visible viewport metrics from the same target.
pub async fn cdp_set_viewport_size(
    endpoint: &str,
    target_id: &str,
    width: u32,
    height: u32,
    device_scale_factor: f64,
) -> A11yResult<CdpViewportResult> {
    validate_viewport_override(width, height, device_scale_factor)?;
    let requested = CdpViewportOverride {
        width,
        height,
        device_scale_factor,
        mobile: false,
    };
    run_device_metrics_command(
        endpoint,
        target_id,
        DeviceMetricsCommand::Set(requested.clone()),
    )
    .await?;
    let readback = viewport_readback(endpoint, target_id).await?;
    Ok(CdpViewportResult {
        endpoint: endpoint.to_owned(),
        cdp_target_id: readback.target_id,
        operation: "set".to_owned(),
        requested: Some(requested),
        page_url: readback.url,
        page_title: readback.title,
        ready_state: readback.ready_state,
        readback: readback.metrics,
    })
}

/// Clears `Emulation.setDeviceMetricsOverride` for one CDP page target, then
/// reads back the real page-visible viewport metrics from that target.
pub async fn cdp_reset_viewport_size(
    endpoint: &str,
    target_id: &str,
) -> A11yResult<CdpViewportResult> {
    run_device_metrics_command(endpoint, target_id, DeviceMetricsCommand::Reset).await?;
    let readback = viewport_readback(endpoint, target_id).await?;
    Ok(CdpViewportResult {
        endpoint: endpoint.to_owned(),
        cdp_target_id: readback.target_id,
        operation: "reset".to_owned(),
        requested: None,
        page_url: readback.url,
        page_title: readback.title,
        ready_state: readback.ready_state,
        readback: readback.metrics,
    })
}

fn validate_viewport_override(width: u32, height: u32, device_scale_factor: f64) -> A11yResult<()> {
    if width == 0 || width > CDP_DEVICE_METRICS_MAX_DIMENSION {
        return Err(A11yError::CdpAxtreeFailed {
            detail: format!(
                "viewport width must be 1..={CDP_DEVICE_METRICS_MAX_DIMENSION}, got {width}"
            ),
        });
    }
    if height == 0 || height > CDP_DEVICE_METRICS_MAX_DIMENSION {
        return Err(A11yError::CdpAxtreeFailed {
            detail: format!(
                "viewport height must be 1..={CDP_DEVICE_METRICS_MAX_DIMENSION}, got {height}"
            ),
        });
    }
    if !device_scale_factor.is_finite()
        || device_scale_factor <= 0.0
        || device_scale_factor > CDP_DEVICE_SCALE_FACTOR_MAX
    {
        return Err(A11yError::CdpAxtreeFailed {
            detail: format!(
                "device_scale_factor must be finite and in 0..={CDP_DEVICE_SCALE_FACTOR_MAX}, got {device_scale_factor}"
            ),
        });
    }
    Ok(())
}

async fn run_device_metrics_command(
    endpoint: &str,
    target_id: &str,
    command: DeviceMetricsCommand,
) -> A11yResult<()> {
    use chromiumoxide::Browser;
    use chromiumoxide::cdp::browser_protocol::emulation::{
        ClearDeviceMetricsOverrideParams, SetDeviceMetricsOverrideParams,
    };
    use futures_util::StreamExt as _;

    let (browser, mut handler) =
        Browser::connect(endpoint)
            .await
            .map_err(|err| A11yError::CdpAttachFailed {
                detail: format!("connect {endpoint}: {err}"),
            })?;
    let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });

    let result = async {
        let page = crate::cdp_action::get_target_page_with_discovery(&browser, target_id).await?;
        match command {
            DeviceMetricsCommand::Set(override_metrics) => {
                let params = SetDeviceMetricsOverrideParams::builder()
                    .width(i64::from(override_metrics.width))
                    .height(i64::from(override_metrics.height))
                    .device_scale_factor(override_metrics.device_scale_factor)
                    .mobile(override_metrics.mobile)
                    .screen_width(i64::from(override_metrics.width))
                    .screen_height(i64::from(override_metrics.height))
                    .build()
                    .map_err(|err| A11yError::CdpAxtreeFailed {
                        detail: format!("Emulation.setDeviceMetricsOverride params: {err}"),
                    })?;
                page.execute(params)
                    .await
                    .map_err(|err| A11yError::CdpAxtreeFailed {
                        detail: format!("Emulation.setDeviceMetricsOverride: {err}"),
                    })?;
            }
            DeviceMetricsCommand::Reset => {
                page.execute(ClearDeviceMetricsOverrideParams::default())
                    .await
                    .map_err(|err| A11yError::CdpAxtreeFailed {
                        detail: format!("Emulation.clearDeviceMetricsOverride: {err}"),
                    })?;
            }
        }
        Ok(())
    }
    .await;

    handler_task.abort();
    result
}

struct ViewportReadback {
    target_id: String,
    url: String,
    title: String,
    ready_state: String,
    metrics: CdpViewportReadback,
}

async fn viewport_readback(endpoint: &str, target_id: &str) -> A11yResult<ViewportReadback> {
    let evaluated = crate::cdp_action::cdp_evaluate_expression(
        endpoint,
        target_id,
        VIEWPORT_READBACK_JS,
        false,
        true,
    )
    .await?;
    let metrics =
        serde_json::from_value::<CdpViewportReadback>(evaluated.value).map_err(|error| {
            A11yError::CdpAxtreeFailed {
                detail: format!("viewport metrics readback decode: {error}"),
            }
        })?;
    Ok(ViewportReadback {
        target_id: evaluated.target_id,
        url: evaluated.url,
        title: evaluated.title,
        ready_state: evaluated.ready_state,
        metrics,
    })
}

const VIEWPORT_READBACK_JS: &str = r#"(() => {
  const viewport = globalThis.visualViewport || null;
  return {
    inner_width: Math.round(globalThis.innerWidth || 0),
    inner_height: Math.round(globalThis.innerHeight || 0),
    device_pixel_ratio: Number(globalThis.devicePixelRatio || 0),
    screen_width: Math.round(globalThis.screen ? globalThis.screen.width || 0 : 0),
    screen_height: Math.round(globalThis.screen ? globalThis.screen.height || 0 : 0),
    outer_width: Math.round(globalThis.outerWidth || 0),
    outer_height: Math.round(globalThis.outerHeight || 0),
    visual_viewport_width: viewport ? Number(viewport.width) : null,
    visual_viewport_height: viewport ? Number(viewport.height) : null
  };
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_override_validation_edges() {
        assert!(validate_viewport_override(1280, 720, 1.25).is_ok());
        assert!(validate_viewport_override(0, 720, 1.0).is_err());
        assert!(validate_viewport_override(1280, 0, 1.0).is_err());
        assert!(
            validate_viewport_override(CDP_DEVICE_METRICS_MAX_DIMENSION + 1, 720, 1.0).is_err()
        );
        assert!(validate_viewport_override(1280, 720, 0.0).is_err());
        assert!(validate_viewport_override(1280, 720, f64::NAN).is_err());
    }
}
