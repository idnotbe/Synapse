use rmcp::ErrorData;
use synapse_core::error_codes;

use crate::m1::mcp_error;

pub(crate) const MIN_AUTO_WAIT_TIMEOUT_MS: u32 = 50;
pub(crate) const MAX_AUTO_WAIT_TIMEOUT_MS: u32 = 30_000;
const DEFAULT_AUTO_WAIT_TIMEOUT_MS: u32 = 2_000;

pub(crate) const fn default_auto_wait_timeout_ms() -> u32 {
    DEFAULT_AUTO_WAIT_TIMEOUT_MS
}

pub(crate) fn validate_auto_wait_timeout(
    tool: &str,
    enabled: bool,
    timeout_ms: u32,
) -> Result<(), ErrorData> {
    if !enabled {
        return Ok(());
    }
    if !(MIN_AUTO_WAIT_TIMEOUT_MS..=MAX_AUTO_WAIT_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!(
                "{tool} auto_wait_timeout_ms must be in {MIN_AUTO_WAIT_TIMEOUT_MS}..={MAX_AUTO_WAIT_TIMEOUT_MS}, got {timeout_ms}"
            ),
        ));
    }
    Ok(())
}
