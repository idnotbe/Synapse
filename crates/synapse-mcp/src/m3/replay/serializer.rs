use rmcp::ErrorData;
use serde::Serialize;
use synapse_core::error_codes;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::m1::mcp_error;

pub(super) async fn write_json_line<W, T>(writer: &mut W, value: &T) -> Result<(), ErrorData>
where
    W: AsyncWrite + Unpin + Send,
    T: Serialize + Sync + ?Sized,
{
    let line = serde_json::to_vec(value).map_err(|error| {
        mcp_error(
            error_codes::TOOL_INTERNAL_ERROR,
            format!("replay_record could not serialize record: {error}"),
        )
    })?;
    writer
        .write_all(&line)
        .await
        .map_err(|error| write_error(&error))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|error| write_error(&error))
}

fn write_error(error: &std::io::Error) -> ErrorData {
    mcp_error(
        error_codes::TOOL_INTERNAL_ERROR,
        format!("replay_record could not write JSONL record: {error}"),
    )
}
