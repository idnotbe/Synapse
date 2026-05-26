use rmcp::ErrorData;
use synapse_core::Event;
use tokio::io::AsyncWrite;

use super::{ReplayRecordLine, ReplayTarget, serializer::write_json_line};

pub(super) async fn drain_events<W>(
    writer: &mut W,
    events: Vec<Event>,
    target: ReplayTarget,
) -> Result<u64, ErrorData>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut records_written = 0_u64;
    for event in events {
        match target {
            ReplayTarget::Events => write_json_line(writer, &event).await?,
            ReplayTarget::Both => {
                write_json_line(writer, &ReplayRecordLine::Event { record: &event }).await?;
            }
            ReplayTarget::Observations => {}
        }
        records_written = records_written.saturating_add(1);
    }
    Ok(records_written)
}
