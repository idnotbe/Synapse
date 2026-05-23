use anyhow::Context;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use synapse_core::error_codes;
use synapse_test_utils::stdio_mcp_client::StdioMcpClient;
use tempfile::TempDir;

#[tokio::test]
async fn act_pad_schema_defaults_recording_and_edges_fsv() -> anyhow::Result<()> {
    let log_dir = TempDir::new()?;
    let mut client = StdioMcpClient::launch_and_init_with_env(
        Some(log_dir.path()),
        &[("SYNAPSE_MCP_RECORDING_BACKEND", "1")],
    )
    .await?;
    let resp = client.tools_list().await?;
    let tools = resp
        .get("tools")
        .and_then(Value::as_array)
        .context("tools array missing")?;
    assert_act_pad_schema(tools)?;
    call_act_pad_happy_and_edges(&mut client).await?;

    assert!(client.shutdown().await?.success());
    let logs = read_logs(log_dir.path())?;
    assert_recording_log_readbacks(&logs)?;
    Ok(())
}

fn assert_act_pad_schema(tools: &[Value]) -> anyhow::Result<()> {
    let act_pad = tools
        .iter()
        .find(|tool| tool.get("name") == Some(&Value::String("act_pad".to_owned())))
        .context("act_pad tool missing")?;
    let schema = &act_pad["inputSchema"];
    println!(
        "source_of_truth=tools_list tool=act_pad edge=schema before=tool_count:{}",
        tools.len()
    );
    println!(
        "source_of_truth=tools_list tool=act_pad edge=defaults after=pad_id:{} controller:{} backend:{} additionalProperties:{}",
        schema["properties"]["pad_id"]["default"],
        schema["properties"]["controller"]["default"],
        schema["properties"]["backend"]["default"],
        schema["additionalProperties"]
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["pad_id"]["default"], 0);
    assert_eq!(schema["properties"]["controller"]["default"], "x360");
    assert_eq!(schema["properties"]["backend"]["default"], "vigem");
    assert_eq!(schema["required"], json!(["report"]));
    assert_report_schema(schema);

    let projection = json!({
        "name": act_pad["name"],
        "description": act_pad["description"],
        "inputSchema": act_pad["inputSchema"],
        "outputSchemaRoot": schema_root(act_pad.get("outputSchema")),
    });
    insta::assert_json_snapshot!("m2_act_pad_tool", projection);
    Ok(())
}

fn assert_report_schema(schema: &Value) {
    let schema_text = schema.to_string();
    assert!(schema_text.contains("\"ActPadReport\""));
    assert!(schema_text.contains("\"buttons\""));
    assert!(schema_text.contains("\"thumb_l\""));
    assert!(schema_text.contains("\"thumb_r\""));
    assert!(schema_text.contains("\"minimum\":-1.0"));
    assert!(schema_text.contains("\"maximum\":1.0"));
    assert!(schema_text.contains("\"minimum\":0.0"));
    assert!(schema_text.contains("\"vigem\""));
    assert!(schema_text.contains("\"hardware\""));
    assert!(schema_text.contains("\"x360\""));
    assert!(schema_text.contains("\"ds4\""));
    assert!(!schema_text.contains("\"guide\""));
}

async fn call_act_pad_happy_and_edges(client: &mut StdioMcpClient) -> anyhow::Result<()> {
    println!(
        "source_of_truth=mcp_act_pad edge=report before=pad_id:0 buttons:[a,start] thumb_l:[0.5,-0.5] lt:0.25"
    );
    let report = client
        .tools_call(
            "act_pad",
            json!({
                "report": {
                    "buttons": ["a", "start"],
                    "thumb_l": [0.5, -0.5],
                    "lt": 0.25
                }
            }),
        )
        .await?;
    let response: ActPadWireResponse = structured(&report)?;
    println!(
        "source_of_truth=mcp_act_pad edge=report after=ok:{} pad_id:{} controller:{} buttons:{:?} backend_used:{} hold_ms:{:?} returned_to_neutral:{} elapsed_ms:{} expected_sequence:pad_report:pad=0:controller=x360:buttons=a+start:thumb_l=(0.500,-0.500):thumb_r=(0.000,0.000):lt=0.250:rt=0.000",
        response.ok,
        response.pad_id,
        response.controller,
        response.buttons,
        response.backend_used,
        response.hold_ms,
        response.returned_to_neutral,
        response.elapsed_ms
    );
    assert!(response.ok);
    assert_eq!(response.pad_id, 0);
    assert_eq!(response.controller, "x360");
    assert_eq!(response.buttons, ["a", "start"]);
    assert_eq!(response.backend_used, "vigem");
    assert_eq!(response.hold_ms, None);
    assert!(!response.returned_to_neutral);

    println!("source_of_truth=mcp_act_pad edge=hold_neutral before=pad_id:2 buttons:[b] hold_ms:1");
    let hold = client
        .tools_call(
            "act_pad",
            json!({
                "pad_id": 2,
                "report": {"buttons": ["b"]},
                "hold_ms": 1
            }),
        )
        .await?;
    let response: ActPadWireResponse = structured(&hold)?;
    println!(
        "source_of_truth=mcp_act_pad edge=hold_neutral after=ok:{} pad_id:{} controller:{} buttons:{:?} backend_used:{} hold_ms:{:?} returned_to_neutral:{} elapsed_ms:{} expected_sequence:pad_report:pad=2:controller=x360:buttons=b:...>pad_report:pad=2:controller=x360:buttons=none:...",
        response.ok,
        response.pad_id,
        response.controller,
        response.buttons,
        response.backend_used,
        response.hold_ms,
        response.returned_to_neutral,
        response.elapsed_ms
    );
    assert!(response.ok);
    assert_eq!(response.pad_id, 2);
    assert_eq!(response.controller, "x360");
    assert_eq!(response.buttons, ["b"]);
    assert_eq!(response.hold_ms, Some(1));
    assert!(response.returned_to_neutral);

    call_act_pad_ds4_happy(client).await?;
    call_act_pad_error_edges(client).await
}

async fn call_act_pad_ds4_happy(client: &mut StdioMcpClient) -> anyhow::Result<()> {
    println!(
        "source_of_truth=mcp_act_pad edge=ds4_report before=pad_id:3 controller:ds4 buttons:[x,y] thumb_r:[-1,1] rt:1"
    );
    let ds4 = client
        .tools_call(
            "act_pad",
            json!({
                "pad_id": 3,
                "controller": "ds4",
                "report": {
                    "buttons": ["x", "y"],
                    "thumb_r": [-1.0, 1.0],
                    "rt": 1.0
                }
            }),
        )
        .await?;
    let response: ActPadWireResponse = structured(&ds4)?;
    println!(
        "source_of_truth=mcp_act_pad edge=ds4_report after=ok:{} pad_id:{} controller:{} buttons:{:?} backend_used:{} hold_ms:{:?} returned_to_neutral:{} elapsed_ms:{} expected_sequence:pad_report:pad=3:controller=ds4:buttons=x+y:thumb_l=(0.000,0.000):thumb_r=(-1.000,1.000):lt=0.000:rt=1.000",
        response.ok,
        response.pad_id,
        response.controller,
        response.buttons,
        response.backend_used,
        response.hold_ms,
        response.returned_to_neutral,
        response.elapsed_ms
    );
    assert!(response.ok);
    assert_eq!(response.pad_id, 3);
    assert_eq!(response.controller, "ds4");
    assert_eq!(response.buttons, ["x", "y"]);
    assert_eq!(response.backend_used, "vigem");
    assert_eq!(response.hold_ms, None);
    assert!(!response.returned_to_neutral);
    Ok(())
}

async fn call_act_pad_error_edges(client: &mut StdioMcpClient) -> anyhow::Result<()> {
    assert_error_code(
        client,
        "thumb_l_out_of_range",
        "thumb_l:[1.5,0]",
        json!({"report": {"thumb_l": [1.5, 0]}}),
        error_codes::TOOL_PARAMS_INVALID,
    )
    .await?;
    assert_error_code(
        client,
        "trigger_out_of_range",
        "lt:2.0",
        json!({"report": {"lt": 2.0}}),
        error_codes::TOOL_PARAMS_INVALID,
    )
    .await?;
    assert_error_code(
        client,
        "controller_invalid",
        "controller:dualshock",
        json!({"controller": "dualshock", "report": {"buttons": ["a"]}}),
        error_codes::TOOL_PARAMS_INVALID,
    )
    .await?;
    assert_error_code(
        client,
        "hardware_backend",
        "backend:hardware",
        json!({"report": {"buttons": ["a"]}, "backend": "hardware"}),
        error_codes::ACTION_BACKEND_UNAVAILABLE,
    )
    .await?;
    assert_error_code(
        client,
        "hold_zero",
        "hold_ms:0",
        json!({"report": {"buttons": ["a"]}, "hold_ms": 0}),
        error_codes::TOOL_PARAMS_INVALID,
    )
    .await?;
    assert_error_code(
        client,
        "hold_exceeded",
        "hold_ms:30001",
        json!({"report": {"buttons": ["a"]}, "hold_ms": 30001}),
        error_codes::ACTION_HOLD_EXCEEDED_MAX,
    )
    .await?;
    assert_error_code(
        client,
        "extra_property",
        "junk:true",
        json!({"report": {"buttons": ["a"]}, "junk": true}),
        error_codes::TOOL_PARAMS_INVALID,
    )
    .await
}

async fn assert_error_code(
    client: &mut StdioMcpClient,
    edge: &str,
    before: &str,
    args: Value,
    expected_code: &'static str,
) -> anyhow::Result<()> {
    println!("source_of_truth=mcp_act_pad edge={edge} before={before}");
    let error = client.tools_call_error("act_pad", args).await?;
    println!("source_of_truth=mcp_act_pad edge={edge} after={error}");
    assert_eq!(error_code(&error), Some(expected_code));
    Ok(())
}

fn assert_recording_log_readbacks(logs: &str) -> anyhow::Result<()> {
    let readbacks = recording_readbacks(logs)?;
    assert_readback(
        &readbacks,
        "report",
        "pad_report:pad=0:controller=x360:buttons=a+start:thumb_l=(0.500,-0.500):thumb_r=(0.000,0.000):lt=0.250:rt=0.000",
        1,
        "0:controller=x360:buttons=a+start:thumb_l=(0.500,-0.500):thumb_r=(0.000,0.000):lt=0.250:rt=0.000",
    )?;
    assert_readback(
        &readbacks,
        "hold_neutral",
        "pad_report:pad=2:controller=x360:buttons=b:thumb_l=(0.000,0.000):thumb_r=(0.000,0.000):lt=0.000:rt=0.000>pad_report:pad=2:controller=x360:buttons=none:thumb_l=(0.000,0.000):thumb_r=(0.000,0.000):lt=0.000:rt=0.000",
        2,
        "0:controller=x360:buttons=a+start:thumb_l=(0.500,-0.500):thumb_r=(0.000,0.000):lt=0.250:rt=0.000",
    )?;
    assert_readback(
        &readbacks,
        "ds4_report",
        "pad_report:pad=3:controller=ds4:buttons=x+y:thumb_l=(0.000,0.000):thumb_r=(-1.000,1.000):lt=0.000:rt=1.000",
        1,
        "0:controller=x360:buttons=a+start:thumb_l=(0.500,-0.500):thumb_r=(0.000,0.000):lt=0.250:rt=0.000|3:controller=ds4:buttons=x+y:thumb_l=(0.000,0.000):thumb_r=(-1.000,1.000):lt=0.000:rt=1.000",
    )?;
    println!(
        "source_of_truth=recording_log tool=act_pad edge=failed_edges after_readback_count={} expected_successful_readbacks=3",
        readbacks.len()
    );
    assert_eq!(readbacks.len(), 3);
    Ok(())
}

fn assert_readback(
    readbacks: &[RecordingReadback],
    edge: &str,
    expected_sequence: &str,
    expected_count: u64,
    expected_pad_state: &str,
) -> anyhow::Result<()> {
    let readback = readbacks
        .iter()
        .find(|readback| {
            readback.event_sequence == expected_sequence
                && readback.new_event_count == expected_count
                && readback.pad_state == expected_pad_state
        })
        .with_context(|| format!("{edge} act_pad recording readback missing expected state"))?;
    println!(
        "source_of_truth=recording_log tool=act_pad edge={edge} after_event_sequence={} new_event_count={} pad_state={}",
        readback.event_sequence, readback.new_event_count, readback.pad_state
    );
    Ok(())
}

#[derive(serde::Deserialize)]
struct ActPadWireResponse {
    ok: bool,
    pad_id: u8,
    controller: String,
    buttons: Vec<String>,
    backend_used: String,
    hold_ms: Option<u32>,
    returned_to_neutral: bool,
    elapsed_ms: u32,
}

#[derive(Debug)]
struct RecordingReadback {
    event_sequence: String,
    new_event_count: u64,
    pad_state: String,
}

fn structured<T: DeserializeOwned>(resp: &Value) -> anyhow::Result<T> {
    serde_json::from_value(resp["structuredContent"].clone()).context("decode structuredContent")
}

fn error_code(error: &Value) -> Option<&str> {
    error
        .get("data")
        .and_then(|data| data.get("code"))
        .and_then(Value::as_str)
}

fn schema_root(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "title": value.get("title"),
        "type": value.get("type"),
        "required": value.get("required"),
        "additionalProperties": value.get("additionalProperties"),
    })
}

fn read_logs(path: &std::path::Path) -> anyhow::Result<String> {
    let mut logs = String::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.metadata()?.is_file() {
            logs.push_str(&std::fs::read_to_string(entry.path())?);
        }
    }
    Ok(logs)
}

fn recording_readbacks(logs: &str) -> anyhow::Result<Vec<RecordingReadback>> {
    let mut readbacks = Vec::new();
    for line in logs.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)?;
        let fields = &value["fields"];
        if fields.get("code").and_then(Value::as_str) != Some("M2_ACT_PAD_RECORDING_READBACK") {
            continue;
        }
        let event_sequence = fields
            .get("event_sequence")
            .and_then(Value::as_str)
            .context("recording readback missing event_sequence")?
            .to_owned();
        let new_event_count = fields
            .get("new_event_count")
            .and_then(Value::as_u64)
            .context("recording readback missing new_event_count")?;
        let pad_state = fields
            .get("pad_state")
            .and_then(Value::as_str)
            .context("recording readback missing pad_state")?
            .to_owned();
        readbacks.push(RecordingReadback {
            event_sequence,
            new_event_count,
            pad_state,
        });
    }
    Ok(readbacks)
}
