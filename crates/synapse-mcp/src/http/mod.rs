mod auth;
mod session;
pub mod sse;
mod transport;

use crate::m3::M3ServiceConfig;

pub async fn serve(
    bind: &str,
    allow_non_loopback: bool,
    m3_config: M3ServiceConfig,
) -> anyhow::Result<std::process::ExitCode> {
    transport::serve(bind, allow_non_loopback, m3_config).await
}
