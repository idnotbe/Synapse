# Synapse

[![Software Input + M5 Registry/Audit Moat](https://img.shields.io/badge/status-software_input_%2B_M5_registry_audit_moat-blue)](https://github.com/ChrisRoyse/Synapse/issues/588)

Synapse is a Rust MCP server that gives AI agents a local computer-use body:
structured perception, action, and low-latency reflexes live in Synapse while the
connected model stays the brain. It speaks the Model Context Protocol over stdio
or loopback HTTP, so it drops into Claude Code, Codex, and the Claude Desktop app
as a tool server. Synapse is Windows-native (Win32 `SendInput`, UI Automation,
WGC/DXGI capture, ViGEmBus virtual controllers).

## Install Synapse (paste this prompt to your AI agent)

Open Claude Code, Codex, or any coding agent **on the Windows machine you want
Synapse to control**, then paste this prompt:

```text
Install Synapse for me and wire it into my AI tools.

1. Clone the repo: git clone https://github.com/ChrisRoyse/Synapse.git
   (cd into it; if it already exists, git pull instead).
2. Build and install the MCP server globally with cargo:
     cargo install --path crates/synapse-mcp --force
   This drops synapse-mcp.exe into my Cargo bin dir
   (%USERPROFILE%\.cargo\bin\synapse-mcp.exe). Find the absolute path to that
   binary and use it verbatim in every config below.
3. Connect it to Claude Code (user scope):
     claude mcp add --scope user synapse -- "<absolute path>\synapse-mcp.exe" --mode stdio
4. Connect it to Codex by adding this to ~/.codex/config.toml:
     [mcp_servers.synapse]
     command = "<absolute path>\\synapse-mcp.exe"
     args = ["--mode", "stdio"]
5. Connect it to the Claude Desktop app by adding a "synapse" server to
   %APPDATA%\Claude\claude_desktop_config.json under "mcpServers" with the same
   command and ["--mode","stdio"] args. Preserve any existing servers in that file.
6. Verify: restart each client, then call the Synapse `health` tool and confirm
   it returns { "ok": true, ... }.

I'm on Windows. Use the real absolute Cargo bin path, don't invent one, and tell
me anything that needs my approval (e.g. installing the Rust toolchain).
```

The agent needs a stable Rust toolchain (`rustup` / `cargo`). If `cargo` is
missing, let the agent install it, or grab it from <https://rustup.rs> first.

## What's left on the docket

M0–M4 are complete and tagged (`v0.1.0-m0` … `v0.1.0-m4`); the M1 and M2
milestones are fully closed (49 and 82 issues). Active work is **M5 — Production
polish**, which has two threads:

1. **Profile-registry / audit-data moat** — the compounding learning loop
   (profile used → runtime outcome audited → quality/compatibility learned →
   profile improved → registry distributes better profile → more evidence).
   Strategy lives in [issue #454](https://github.com/ChrisRoyse/Synapse/issues/454),
   child work in #455–#470. The registry, audit-export, profile-authoring, and
   quality tools are already live in the tool surface.
2. **Whole-body stress & showcase campaign** —
   [issue #594](https://github.com/ChrisRoyse/Synapse/issues/594) and its
   children prove every Synapse tool under load and in real end-to-end demos.
   These are the bulk of the open queue:

   | Open scenarios | Issues |
   |---|---|
   | Stress / torture (UIA fanout, capture thrash, OCR, detection, audio, SendInput rate-limit, combo precision, drag, ViGEm sweep, clipboard, soak, DPI) | #595–#604, #633, #634 |
   | Showcase end-to-end demos (Paint art-bot, real game session, voice-reactive reflex, Rube Goldberg chain, browser marathon) | #628–#632 |
   | EverQuest full-loop + autocombat soak — **blocked** on an operator-only Daybreak EULA/account decision; all reversible work is done | #624, #625 |

The physical-HID strategy is retired by the software-only input decision in
[issue #588](https://github.com/ChrisRoyse/Synapse/issues/588). The active
architecture direction is **delta-first reality** ([#536](https://github.com/ChrisRoyse/Synapse/issues/536)):
Synapse feeds the agent ordered changes after a baseline snapshot, then
periodically audits the accumulated assumption against full physical reality and
forces a rebase when drift is found (live via `reality_baseline`,
`observe_delta`, `reality_audit`).

## Capabilities

Synapse exposes **79 live MCP tools**. The full registry is in
[docs/computergames/05_mcp_tool_surface.md](docs/computergames/05_mcp_tool_surface.md)
and [docs/systemspec/13_mcp_tool_reference.md](docs/systemspec/13_mcp_tool_reference.md).
At a glance:

- **Perception** — `observe`, `find`, `read_text` (OCR), `audio_tail`,
  `audio_transcribe`, `subscribe`, plus `set_capture_target` /
  `set_perception_mode` to steer A11y/pixel/hybrid capture.
- **Delta-first reality** — `reality_baseline`, `observe_delta`, `reality_audit`.
- **Action** — `act_click`, `act_type`, `act_press`, `act_keymap`, `act_aim`,
  `act_drag`, `act_scroll`, `act_pad` (gamepad), `act_clipboard`, `act_combo`
  (timed sequences), `act_run_shell`, `act_launch`, and `release_all`.
- **Reflexes** — low-latency in-process triggers: `reflex_register`,
  `reflex_cancel`, `reflex_list`, `reflex_history`.
- **Profiles & registry/audit moat** — `profile_list`, `profile_activate`,
  the `profile_authoring_*`, `profile_registry_*`, `profile_quality_refresh`,
  and `audit_*` tool families.
- **Storage / health** — `health`, `storage_inspect`, `storage_gc_once`,
  `replay_record`, and probe tools.
- **EverQuest domain pack** — an `everquest_*` family (state, memory, planner
  guard, route plan, trajectory/episode export, ContextGraph bridge, predictive
  model, surprise detection, action-prior scorecard) demonstrating a full
  perception→memory→planner→learning loop.

The M1 starter surface, for quick orientation:

| Tool | Description | Milestone | Status |
|---|---|---:|---|
| `health` | Reports server version, build, uptime, and subsystem health. | [M0](https://github.com/ChrisRoyse/Synapse/milestone/1) | Done |
| `observe` | Returns the current structured perception snapshot. | [M1](https://github.com/ChrisRoyse/Synapse/milestone/2) | Done |
| `find` | Searches accessible elements and detected entities by role/name/query. | [M1](https://github.com/ChrisRoyse/Synapse/milestone/2) | Done |
| `read_text` | Reads OCR text from a region or element target. | [M1](https://github.com/ChrisRoyse/Synapse/milestone/2) | Done |
| `set_capture_target` | Sets the active primary, monitor, window, or element-window capture target. | [M1](https://github.com/ChrisRoyse/Synapse/milestone/2) | Done |
| `set_perception_mode` | Overrides perception mode between auto, a11y-only, pixel-only, and hybrid. | [M1](https://github.com/ChrisRoyse/Synapse/milestone/2) | Done |

## Build

Use the current installed stable Rust toolchain. The repository is verified with
Rust 1.95 and intentionally does not pin an older toolchain.

```bash
cargo build --release --workspace
```

The release binary is written to `target/release/synapse-mcp` (`synapse-mcp.exe`
on Windows). To install it globally (the path the install prompt uses):

```bash
cargo install --path crates/synapse-mcp --force
```

This places the binary in your Cargo bin directory
(`%USERPROFILE%\.cargo\bin\synapse-mcp.exe` on Windows).

## Input Backends

Synapse ships two live input backends:

| Backend | Purpose |
|---|---|
| `software` | Keyboard and mouse through Win32 `SendInput`; default for keyboard, mouse, click, type, aim, drag, scroll, combo, and release-all paths. |
| `vigem` | Software-only virtual Xbox/DS4 controller reports through ViGEmBus; default for pad actions. |

The legacy `hardware` backend token still parses for profile/package
compatibility, but it is not a live backend. Requests that resolve to `hardware`
fail closed with `ACTION_BACKEND_UNAVAILABLE` and guidance to use `software` or
`vigem`.

## Run

For MCP clients, run stdio mode (this is what the client configs below launch):

```bash
synapse-mcp --mode stdio
```

For a process/socket/log source of truth under repo control, run the loopback
HTTP transport with an isolated DB and log directory:

```bash
SYNAPSE_BEARER_TOKEN=local-token synapse-mcp --mode http --bind 127.0.0.1:7700 --db .runs/issue/db
```

Inspect available flags:

```bash
synapse-mcp --help
```

## Configure MCP Clients (manual)

The install prompt above does this for you. To wire it up by hand, point each
client at the installed `synapse-mcp` binary in stdio mode. Substitute your real
Cargo bin path for `<cargo-bin>` (Windows: `%USERPROFILE%\.cargo\bin`).

Claude Code (user scope):

```bash
claude mcp add --scope user synapse -- <cargo-bin>\synapse-mcp.exe --mode stdio
```

Codex user config at `~/.codex/config.toml`:

```toml
[mcp_servers.synapse]
command = "C:\\Users\\you\\.cargo\\bin\\synapse-mcp.exe"
args = ["--mode", "stdio"]
```

Claude Desktop on Windows (`%APPDATA%\Claude\claude_desktop_config.json`):

```jsonc
{
  "mcpServers": {
    "synapse": {
      "command": "C:\\Users\\you\\.cargo\\bin\\synapse-mcp.exe",
      "args": ["--mode", "stdio"]
    }
  }
}
```

After the client loads the server, ask it to call the Synapse `health` tool and
confirm the response has the shape shown below.

## Quick Demo

The stdio transport speaks newline-delimited JSON-RPC. A client initializes the
server, sends `notifications/initialized`, then calls a tool:

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
  "subsystems": {
    "action": { "status": "ok" },
    "storage": { "status": "initializing" }
  }
}
```

`uptime_s` is monotonic, subsystem details vary by enabled runtime surface, and
`build` is `dev` unless a build SHA is injected.

## Agent Doctrine

Agents working in this repository must follow [AGENTS.md](AGENTS.md). Manual Full
State Verification on the configured Windows host is the shipping gate. Scripts,
tests, benchmarks, GitHub Actions, and CI are supporting evidence only; they are
never FSV.

When a behavior has a Synapse MCP tool, agents must verify the real `synapse-mcp`
runtime before FSV: process or stdio child, bind/socket, authenticated `health`,
initialized MCP session, and `tools/list`. The trigger must be the real MCP
`tools/call`, followed by a separate read of the physical source of truth such as
RocksDB rows, file bytes, UI state, logs, or device state. Tool return values and
`health` are liveness/attempt evidence only.

Missing local tools, drivers, models, devices, files, or services are
acquisition/setup work, not blockers. Agents use Synapse and normal OS/shell/
browser/package-manager workflows to make the missing thing real, then read the
physical source of truth directly. Nothing is `status:blocked` because a
configured-host prerequisite is absent; the only blockable item is the exact
operator-only, hard-to-reverse external action left after every reversible local
step is exhausted.

The profile-registry / audit-data moat governance lives in
[docs/computergames/20_profile_registry_governance.md](docs/computergames/20_profile_registry_governance.md)
(contribution, attribution, provenance, licensing, consent, revocation), with the
shared-registry protocol in
[21_profile_registry_protocol.md](docs/computergames/21_profile_registry_protocol.md),
the local storage model in
[22_profile_registry_data_model.md](docs/computergames/22_profile_registry_data_model.md),
and package manifests in
[23_profile_package_manifest.md](docs/computergames/23_profile_package_manifest.md).
Local registry use stays offline-capable and account-free.

## Documentation Map

- Product and architecture PRD: [docs/computergames/README.md](docs/computergames/README.md)
- Implementation plan: [docs/impplan/README.md](docs/impplan/README.md)
- MCP tool surface: [docs/computergames/05_mcp_tool_surface.md](docs/computergames/05_mcp_tool_surface.md)
- MCP runtime FSV path: [docs/computergames/25_mcp_runtime_fsv_path.md](docs/computergames/25_mcp_runtime_fsv_path.md)
- Current Rust/dependency decision: [docs/adr/0001-current-rust-and-dependencies.md](docs/adr/0001-current-rust-and-dependencies.md)

## License

Synapse is licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
