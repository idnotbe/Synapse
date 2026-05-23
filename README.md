# Synapse

[![M0 Bootstrap](https://img.shields.io/badge/status-M0_bootstrap-blue)](https://github.com/ChrisRoyse/Synapse/milestone/1)

Synapse is a Rust MCP server that gives AI agents a local computer-use body: structured perception, action, and low-latency reflexes live in Synapse while the connected model remains the brain. At M0, the only shipped tool is `health`; perception, action, storage, profiles, and game-control surfaces start in later milestones.

## Status: M0

M0 is the bootstrap milestone: a working `synapse-mcp` binary serves MCP over stdio and answers `tools/call health` with a stable JSON health shape. The live tracker is the [M0 milestone](https://github.com/ChrisRoyse/Synapse/milestone/1), with mission context pinned in [issue #1](https://github.com/ChrisRoyse/Synapse/issues/1). The implementation checklist is [docs/impplan/01_m0_bootstrap.md](docs/impplan/01_m0_bootstrap.md).

## Build

Use the current installed stable Rust toolchain. M0 is currently verified with Rust 1.95; the repository intentionally does not pin an older toolchain.

```bash
cargo build --release --workspace
```

The release binary is written to:

```text
target/release/synapse-mcp
```

On Windows the binary name is `synapse-mcp.exe`.

## Run

For MCP clients, run stdio mode:

```bash
synapse-mcp --mode stdio
```

Inspect available flags:

```bash
synapse-mcp --help
```

The HTTP transport flag is present for the future surface but returns `NOT_YET_IMPLEMENTED` during M0.

## Quick Demo

The stdio transport speaks newline-delimited JSON-RPC. A client initializes the server, sends `notifications/initialized`, then calls the `health` tool:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"manual-demo","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"health","arguments":{}}}
```

The health payload shape is:

```json
{
  "ok": true,
  "version": "0.1.0",
  "build": "dev",
  "uptime_s": 0,
  "subsystems": {}
}
```

`uptime_s` is monotonic and `build` is `dev` unless a build SHA is injected.

## Configure MCP Clients

WSL-global install:

```bash
cargo install --path crates/synapse-mcp --force
```

Codex user config at `~/.codex/config.toml`:

```toml
[mcp_servers.synapse]
command = "/home/cabdru/.cargo/bin/synapse-mcp"
args = ["--mode", "stdio"]
```

Claude Code user config:

```bash
claude mcp add --scope user synapse -- /home/cabdru/.cargo/bin/synapse-mcp --mode stdio
```

Claude Desktop on Windows:

```jsonc
// %APPDATA%\\Claude\\claude_desktop_config.json
{
  "mcpServers": {
    "synapse": {
      "command": "C:\\\\Program Files\\\\Synapse\\\\synapse-mcp.exe",
      "args": ["--mode", "stdio"]
    }
  }
}
```

After the client loads the server, ask it to call the Synapse `health` tool and confirm the response has the shape shown above.

## Documentation Map

- Product and architecture PRD: [docs/computergames/README.md](docs/computergames/README.md)
- Implementation plan: [docs/impplan/README.md](docs/impplan/README.md)
- Current Rust/dependency decision: [docs/adr/0001-current-rust-and-dependencies.md](docs/adr/0001-current-rust-and-dependencies.md)

## License

Synapse is licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
