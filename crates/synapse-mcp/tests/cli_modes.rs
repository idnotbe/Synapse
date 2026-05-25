use std::{net::TcpListener, process::Stdio, time::Duration};

use anyhow::Context;
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::{Child, Command},
};

#[tokio::test]
async fn http_mode_serves_health_until_shutdown() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let bind = free_loopback_bind()?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_synapse-mcp"))
        .args(["--mode", "http", "--bind", &bind])
        .env("SYNAPSE_LOG_DIR", dir.path())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let response = wait_for_health(&bind).await;
    stop_child(&mut child).await?;

    let response = response?;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains(r#""ok":true"#), "{response}");
    Ok(())
}

#[tokio::test]
async fn stdio_mode_reaches_transport_path_on_closed_stdin() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_synapse-mcp"))
        .args(["--mode", "stdio"])
        .env("SYNAPSE_LOG_DIR", dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .context("timed out waiting for stdio closed-stdin exit")??;
    assert!(status.success());

    let mut logs = String::new();
    for entry in std::fs::read_dir(dir.path())? {
        let entry = entry?;
        if entry.metadata()?.is_file() {
            logs.push_str(&std::fs::read_to_string(entry.path())?);
        }
    }
    assert!(logs.contains("MCP_STDIO_STARTED"));
    Ok(())
}

#[tokio::test]
async fn invalid_env_mode_exits_with_clap_error() -> anyhow::Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_synapse-mcp"))
        .env("SYNAPSE_MODE", "garbage")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .await?;

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("invalid value") || stderr.contains("garbage"));
    Ok(())
}

fn free_loopback_bind() -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(addr.to_string())
}

async fn wait_for_health(bind: &str) -> anyhow::Result<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match read_health_once(bind).await {
            Ok(response) => return Ok(response),
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _last_error = error;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error).context("read HTTP health from child"),
        }
    }
}

async fn read_health_once(bind: &str) -> anyhow::Result<String> {
    let mut stream = TcpStream::connect(bind).await?;
    let request = format!("GET /health HTTP/1.1\r\nHost: {bind}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    String::from_utf8(response).context("decode HTTP health response")
}

async fn stop_child(child: &mut Child) -> anyhow::Result<()> {
    child.start_kill().context("stop http-mode child")?;
    tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .context("timed out waiting for http-mode child shutdown")??;
    Ok(())
}
