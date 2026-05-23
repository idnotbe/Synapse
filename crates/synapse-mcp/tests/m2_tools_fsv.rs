use anyhow::Context;
use serde_json::{Value, json};
use synapse_test_utils::stdio_mcp_client::StdioMcpClient;

const EXPECTED_M2_TOOL_NAMES: &[&str] = &[
    "act_aim",
    "act_click",
    "act_clipboard",
    "act_drag",
    "act_pad",
    "act_press",
    "act_scroll",
    "act_type",
    "find",
    "health",
    "observe",
    "read_text",
    "release_all",
    "set_capture_target",
    "set_perception_mode",
];

const M2_ACTION_TOOL_NAMES: &[&str] = &[
    "act_aim",
    "act_click",
    "act_clipboard",
    "act_drag",
    "act_pad",
    "act_press",
    "act_scroll",
    "act_type",
    "release_all",
];

#[tokio::test]
async fn m2_tools_list_contains_exact_sorted_surface_fsv() -> anyhow::Result<()> {
    let mut client = StdioMcpClient::launch_and_init().await?;
    let resp = client.tools_list().await?;
    let tools = resp
        .get("tools")
        .and_then(Value::as_array)
        .context("tools array missing")?;

    let mut names = tools
        .iter()
        .map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .context("tool name missing")
                .map(str::to_owned)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    names.sort();
    println!("source_of_truth=tools_list edge=m2 final_names={names:?}");
    assert_eq!(names, EXPECTED_M2_TOOL_NAMES);

    let m2_action_tools = tools
        .iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| M2_ACTION_TOOL_NAMES.contains(&name))
        })
        .collect::<Vec<_>>();
    assert_eq!(m2_action_tools.len(), M2_ACTION_TOOL_NAMES.len());
    for tool in &m2_action_tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        assert_closed_schema(&tool["inputSchema"], &format!("{name}.inputSchema"));
        if let Some(output) = tool.get("outputSchema") {
            assert_closed_schema(output, &format!("{name}.outputSchema"));
        }
    }
    println!(
        "source_of_truth=schema_closed edge=m2 after=checked_tools:{}",
        m2_action_tools.len()
    );

    let mut projection = tools
        .iter()
        .map(|tool| {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .context("tool name missing")?;
            Ok((
                name.to_owned(),
                json!({
                    "name": tool["name"],
                    "description": tool["description"],
                    "inputSchema": tool["inputSchema"],
                    "outputSchema": tool.get("outputSchema").unwrap_or(&Value::Null),
                }),
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    projection.sort_by(|left, right| left.0.cmp(&right.0));
    let schemas = projection
        .into_iter()
        .map(|(_name, schema)| schema)
        .collect::<Vec<_>>();
    insta::assert_json_snapshot!("m2_tools_list", schemas);

    assert!(client.shutdown().await?.success());
    Ok(())
}

fn assert_closed_schema(value: &Value, path: &str) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("object") {
                assert_eq!(
                    object.get("additionalProperties"),
                    Some(&Value::Bool(false)),
                    "object schema at {path} must set additionalProperties:false"
                );
            }
            for (key, child) in object {
                assert_closed_schema(child, &format!("{path}.{key}"));
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                assert_closed_schema(child, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}
