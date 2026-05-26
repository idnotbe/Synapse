use anyhow::Context;
use serde_json::{Value, json};
use synapse_core::SCHEMA_VERSION;
use synapse_storage::{Db, cf};
use synapse_test_utils::stdio_mcp_client::StdioMcpClient;
use tempfile::TempDir;

#[tokio::test]
async fn storage_tools_are_default_granted_and_persist_probe_rows() -> anyhow::Result<()> {
    let logs = TempDir::new()?;
    let db = TempDir::new()?;
    let db_path = db.path().join("db");
    let db_path_string = db_path.to_string_lossy().into_owned();
    let mut client = StdioMcpClient::launch_and_init_with_env(
        Some(logs.path()),
        &[("SYNAPSE_DB", db_path_string.as_str())],
    )
    .await?;

    let before = structured(&client.tools_call("storage_inspect", json!({})).await?)?;
    let before_events = before["cf_row_counts"][cf::CF_EVENTS].as_u64().unwrap_or(0);
    println!("readback=storage_tool edge=before before_events={before_events}");

    let key_prefix = "regression-default-storage";
    let put = structured(
        &client
            .tools_call(
                "storage_put_probe_rows",
                json!({
                    "cf_name": cf::CF_EVENTS,
                    "key_prefix": key_prefix,
                    "rows": 2,
                    "value_bytes": 16
                }),
            )
            .await?,
    )?;
    assert_eq!(put["rows_added"], 2);
    assert_eq!(put["before_rows"], before_events);
    assert_eq!(put["after_rows"], before_events + 2);

    let after = structured(&client.tools_call("storage_inspect", json!({})).await?)?;
    let after_events = after["cf_row_counts"][cf::CF_EVENTS]
        .as_u64()
        .context("events row count missing")?;
    println!(
        "readback=storage_tool edge=after_mcp before_events={before_events} after_events={after_events} put={put}"
    );
    assert_eq!(after_events, before_events + 2);

    let status = client.shutdown().await?;
    assert!(status.success());

    let durable_rows = direct_probe_row_count(&db_path, key_prefix)?;
    println!(
        "readback=storage_tool edge=after_shutdown_source_of_truth key_prefix={key_prefix} durable_rows={durable_rows}"
    );
    assert_eq!(durable_rows, 2);
    Ok(())
}

fn structured(response: &Value) -> anyhow::Result<Value> {
    if let Some(value) = response.get("structuredContent") {
        return Ok(value.clone());
    }

    let text = response
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
        .context("structured content missing")?;
    serde_json::from_str(text).context("parse text content")
}

fn direct_probe_row_count(db_path: &std::path::Path, key_prefix: &str) -> anyhow::Result<usize> {
    let prefix = key_prefix.as_bytes();
    let db = Db::open(db_path, SCHEMA_VERSION)?;
    Ok(db
        .scan_cf(cf::CF_EVENTS)?
        .into_iter()
        .filter(|(key, _value)| key.starts_with(prefix))
        .count())
}
