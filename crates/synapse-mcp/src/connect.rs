//! `--mode connect`: native stdio<->HTTP bridge to the shared Synapse daemon.
//!
//! Lets a stdio-only MCP client (Claude Desktop, Codex) reach the single shared
//! HTTP daemon instead of spawning its own embedded server (which would contend
//! for the one RocksDB lock). The bridge is a transport-level pump: it forwards
//! raw JSON-RPC between the client's stdio transport and an rmcp
//! Streamable-HTTP client transport pointed at the daemon, so the initialize
//! handshake, `Mcp-Session-Id` sessions, and SSE server->client notifications
//! are all handled by rmcp's client worker. No message interpretation, no
//! external proxy dependency.

use std::{path::Path, process::ExitCode, time::Duration};

use anyhow::Context;
use rmcp::transport::{
    Transport,
    async_rw::AsyncRwTransport,
    streamable_http_client::{StreamableHttpClientTransport, StreamableHttpClientTransportConfig},
};

/// How long to wait for a freshly spawned daemon to become healthy.
const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(15);
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Probe the daemon `/health` endpoint. Returns true only on a 2xx response.
async fn probe_health(bind: &str, token: &str) -> bool {
    let url = format!("http://{bind}/health");
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
    else {
        return false;
    };
    match client.get(&url).bearer_auth(token).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Spawn the shared daemon detached (its own stdio = null so it never writes to
/// the bridge's MCP stdout, and it outlives the bridge). The T1 single-instance
/// guard ensures that if several bridges race to spawn, only one daemon wins.
#[cfg(not(windows))]
fn spawn_detached_daemon(bind: &str, db: Option<&Path>) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("resolve current executable path")?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["--mode", "http", "--bind", bind]);
    if let Some(db) = db {
        cmd.arg("--db").arg(db);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.spawn().context("spawn shared daemon process")?;
    Ok(())
}

/// Spawn the daemon on Windows with `bInheritHandles = FALSE` via
/// `CreateProcessW`. This is critical: `std::process::Command` spawns with
/// handle inheritance enabled, which would leak the stdio pipe handles
/// connecting an MCP client to this bridge into the long-lived daemon — keeping
/// those pipes open so the client could never detect the bridge exiting. With
/// inheritance disabled the detached daemon shares none of our handles.
#[cfg(windows)]
fn spawn_detached_daemon(bind: &str, db: Option<&Path>) -> anyhow::Result<()> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CREATE_NO_WINDOW, CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION, STARTUPINFOW,
    };
    use windows::core::{PCWSTR, PWSTR};

    let exe = std::env::current_exe().context("resolve current executable path")?;
    let mut command_line = String::new();
    command_line.push('"');
    command_line.push_str(&exe.to_string_lossy());
    command_line.push_str("\" --mode http --bind ");
    command_line.push_str(bind);
    if let Some(db) = db {
        command_line.push_str(" --db \"");
        command_line.push_str(&db.to_string_lossy());
        command_line.push('"');
    }
    let mut command_line_w: Vec<u16> =
        command_line.encode_utf16().chain(std::iter::once(0)).collect();

    let mut startup_info = STARTUPINFOW::default();
    startup_info.cb = u32::try_from(core::mem::size_of::<STARTUPINFOW>()).unwrap_or(0);
    let mut process_info = PROCESS_INFORMATION::default();

    // SAFETY: command_line_w is a writable, NUL-terminated UTF-16 buffer kept
    // alive across the call; all optional pointers are null; bInheritHandles is
    // false so the daemon inherits none of this process's handles.
    let result = unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(command_line_w.as_mut_ptr())),
            None,
            None,
            false,
            DETACHED_PROCESS | CREATE_NO_WINDOW,
            None,
            PCWSTR::null(),
            &startup_info,
            &mut process_info,
        )
    };
    result.context("CreateProcessW for shared daemon")?;

    // SAFETY: handles from a successful CreateProcessW; we do not need them.
    unsafe {
        let _ = CloseHandle(process_info.hProcess);
        let _ = CloseHandle(process_info.hThread);
    }
    Ok(())
}

/// Ensure a shared daemon is reachable at `bind`: probe, and if absent spawn one
/// (guarded) and wait until it is healthy. Errors (no fallback) if it never
/// comes up within [`DAEMON_READY_TIMEOUT`].
async fn ensure_daemon_running(bind: &str, db: Option<&Path>, token: &str) -> anyhow::Result<()> {
    if probe_health(bind, token).await {
        tracing::info!(
            code = "MCP_CONNECT_DAEMON_PRESENT",
            bind = %bind,
            "shared daemon already running"
        );
        return Ok(());
    }
    tracing::info!(
        code = "MCP_CONNECT_DAEMON_SPAWNING",
        bind = %bind,
        "no daemon detected; spawning shared daemon"
    );
    spawn_detached_daemon(bind, db).context("spawn shared daemon")?;

    let max_attempts = (DAEMON_READY_TIMEOUT.as_millis() / DAEMON_POLL_INTERVAL.as_millis()) as u32;
    for attempt in 1..=max_attempts {
        tokio::time::sleep(DAEMON_POLL_INTERVAL).await;
        if probe_health(bind, token).await {
            tracing::info!(
                code = "MCP_CONNECT_DAEMON_READY",
                bind = %bind,
                attempts = attempt,
                "spawned daemon is healthy"
            );
            return Ok(());
        }
    }
    anyhow::bail!(
        "MCP_DAEMON_SPAWN_FAILED: shared daemon at {bind} did not become healthy within {}s after spawn",
        DAEMON_READY_TIMEOUT.as_secs()
    );
}

/// Run the stdio<->HTTP bridge against the daemon listening at `bind`
/// (`host:port`). Exits 0 when the client closes stdin or the daemon stream
/// ends.
pub async fn run_connect(bind: &str, db: Option<&Path>) -> anyhow::Result<ExitCode> {
    let uri = format!("http://{bind}/mcp");
    let token = crate::http::load_token_value().context("load daemon bearer token for bridge")?;
    tracing::info!(
        code = "MCP_CONNECT_STARTING",
        daemon_uri = %uri,
        "starting stdio<->http bridge to shared daemon"
    );

    // Ensure exactly one shared daemon is up (spawn it if needed) before bridging.
    ensure_daemon_running(bind, db, &token)
        .await
        .context("ensure shared daemon is running")?;

    let config = StreamableHttpClientTransportConfig::with_uri(uri).auth_header(token);
    let mut daemon = StreamableHttpClientTransport::from_config(config);

    let (stdin, stdout) = rmcp::transport::stdio();
    let mut client = AsyncRwTransport::new_server(stdin, stdout);

    loop {
        tokio::select! {
            from_client = client.receive() => {
                match from_client {
                    Some(message) => daemon
                        .send(message)
                        .await
                        .context("forward client->daemon message")?,
                    None => {
                        tracing::info!(
                            code = "MCP_CONNECT_STDIN_EOF",
                            "client closed stdin; shutting down bridge"
                        );
                        break;
                    }
                }
            }
            from_daemon = daemon.receive() => {
                match from_daemon {
                    Some(message) => client
                        .send(message)
                        .await
                        .context("forward daemon->client message")?,
                    None => {
                        tracing::info!(
                            code = "MCP_CONNECT_DAEMON_CLOSED",
                            "daemon stream closed; shutting down bridge"
                        );
                        break;
                    }
                }
            }
        }
    }

    // Bound shutdown: close() can block (e.g. HTTP session-delete, or a daemon
    // transport whose worker never initialized). Never let cleanup hang the
    // bridge — any lingering rmcp worker task is aborted when the runtime drops
    // on return.
    let _ = tokio::time::timeout(Duration::from_secs(3), daemon.close()).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), client.close()).await;
    Ok(ExitCode::SUCCESS)
}
