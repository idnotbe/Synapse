# 14 — Build and Packaging

## 1. Cargo workspace

`Cargo.toml` at the repo root is the workspace manifest. It declares every member crate, shared dependencies, and build profiles.

```toml
[workspace]
resolver = "2"
members = [
    "crates/synapse-mcp",
    "crates/synapse-core",
    "crates/synapse-capture",
    "crates/synapse-a11y",
    "crates/synapse-perception",
    "crates/synapse-audio",
    "crates/synapse-action",
    "crates/synapse-reflex",
    "crates/synapse-storage",
    "crates/synapse-profiles",
    "crates/synapse-hid-host",
    "crates/synapse-models",
    "crates/synapse-telemetry",
    "crates/synapse-test-utils",
    "crates/synapse-overlay",
]
default-members = ["crates/synapse-mcp", "crates/synapse-overlay"]
exclude = ["firmware/pico-hid"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.83"
license = "MIT OR Apache-2.0"
authors = ["Synapse contributors"]
repository = "https://github.com/<your-org>/synapse"
```

Firmware (`firmware/pico-hid`) is excluded because it targets `thumbv6m-none-eabi`. It is its own Cargo project, built separately.

---

## 2. Workspace dependencies (pinned)

Pinned in `[workspace.dependencies]` so every crate uses the same version. Major dependencies:

```toml
[workspace.dependencies]
# Async / IO
tokio = { version = "1.41", features = ["full"] }
tokio-util = { version = "0.7", features = ["sync"] }
crossbeam = "0.8"
arc-swap = "1.7"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"
toml = "0.8"

# Errors / logging
thiserror = "1.0"
anyhow = "1.0"                        # binary crates only
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"

# Metrics
metrics = "0.23"
metrics-exporter-prometheus = "0.16"
opentelemetry = "0.27"
opentelemetry-otlp = "0.27"

# MCP
rmcp = { version = "1.7", features = ["server", "transport-io", "transport-streamable-http-server", "macros", "schemars"] }

# HTTP
axum = "0.7"
hyper = "1.5"
tower = "0.5"

# Windows specific
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_System_Com",
    "Win32_System_Threading",
    "Win32_UI_Accessibility",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_Graphics_Dxgi",
    "Win32_Graphics_Direct3D11",
    "Media_Ocr",
    "Storage_Streams",
] }
windows-capture = "2.0"
uiautomation = { version = "0.24", features = ["pattern", "control", "event"] }
chromiumoxide = { version = "0.7", features = ["tokio-runtime"] }

# Audio
wasapi = "0.16"

# Input
enigo = "0.6"
vigem-client = "0.1"
serialport = "4.5"

# ML
ort = { version = "2.0", features = ["cuda", "directml"] }

# Storage
rocksdb = { version = "0.22", default-features = false, features = ["lz4", "zstd", "multi-threaded-cf"] }
sled = { version = "0.34", optional = true }

# Utility
clap = { version = "4.5", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.11", features = ["v4", "v7", "serde"] }
schemars = "0.8"
regex = "1.11"
sha2 = "0.10"
crc16 = "0.4"
notify = "6.1"

# Dev / test
proptest = "1.5"
criterion = "0.5"
insta = "1.40"
tempfile = "3.13"
mockall = "0.13"

[workspace.lints.rust]
unsafe_code = "forbid"                # overridden per-crate where needed
unused = "warn"

[workspace.lints.clippy]
all = "deny"
pedantic = "warn"
nursery = "warn"
unwrap_used = "deny"                  # forbid unwrap() outside tests
expect_used = "deny"
```

Per-crate `Cargo.toml` adds `unsafe_code = "allow"` only where needed (`synapse-capture`, `synapse-hid-host` for serial OS handles).

---

## 3. Build profiles

```toml
[profile.dev]
opt-level = 0
debug = "line-tables-only"
incremental = true
lto = false

[profile.release]
opt-level = 3
debug = "limited"
incremental = false
lto = "thin"
codegen-units = 16
strip = "debuginfo"
panic = "abort"

[profile.release-max]
inherits = "release"
codegen-units = 1
lto = "fat"

[profile.bench]
inherits = "release"
debug = "line-tables-only"
```

`release` is the default ship profile. `release-max` is for benchmarks-of-record and when we want the absolute fastest binary. `bench` keeps line-tables for `criterion` flamegraphs.

`panic = "abort"` in release because Synapse's `release_all` runs through the panic hook anyway, and stack unwinding adds binary size without benefit.

---

## 4. Build commands

```powershell
# Standard build
cargo build --release

# Build all examples (FSV harnesses analog)
cargo build --release --examples

# Build only the binary
cargo build --release -p synapse-mcp

# Run tests
cargo test --workspace

# Run with feature flags
cargo build --release --features sled-backend

# Build firmware (separate)
cd firmware/pico-hid
cargo build --release --target thumbv6m-none-eabi
elf2uf2-rs target/thumbv6m-none-eabi/release/synapse-pico-hid synapse-pico-hid.uf2
```

CI runs the matrix listed in `13_testing_strategy.md` §14.

---

## 5. Feature flags

| Flag | Default | Effect |
|---|---|---|
| `rocksdb-backend` | on | Use RocksDB for storage (default) |
| `sled-backend` | off | Use sled instead of RocksDB |
| `cuda` | off | ORT with CUDA execution provider |
| `directml` | on | ORT with DirectML execution provider (default GPU path on Windows) |
| `vlm` | off | Bundle a small VLM for `describe` |
| `perf-profiling` | off | Compile with `tracing-flame` + `pprof` |
| `overlay` | on | Build the debug overlay subbinary |

The default ship build: `rocksdb-backend + directml + overlay`. Operators wanting CUDA pass `--features cuda` at install.

---

## 6. Installation

### 6.1 Via cargo

```powershell
cargo install --git https://github.com/<your-org>/synapse synapse-mcp --features directml
cargo install --git https://github.com/<your-org>/synapse synapse-overlay --features overlay
```

For users with the Rust toolchain installed. Builds from source.

### 6.2 Via prebuilt installer (Windows MSI)

For users without Rust. We ship a signed Windows MSI generated by `wix-installer`:

- `synapse-mcp.exe` installed to `C:\Program Files\Synapse\`
- `synapse-overlay.exe` alongside
- Start menu shortcuts
- Bundled ONNX models for default detection + OCR + STT
- Bundled profiles
- Bundled RP2040 firmware `.uf2`
- Visual C++ runtime redistributable (RocksDB dep)
- Optional checkbox: install ViGEmBus driver (calls Nefarius signed installer)

MSI is signed with the project's code-signing certificate (community-trusted; not on Microsoft's pre-trusted list — operator sees the "Verified Publisher" SmartScreen prompt the first time).

### 6.3 Via winget

```powershell
winget install Synapse.SynapseMCP
```

Published to the community winget repository after v1.0. Refers to the signed MSI.

### 6.4 Via chocolatey

Optional, community-maintained. Same MSI.

### 6.5 Portable zip

For air-gapped installs: a `.zip` containing the binaries + bundled models + profiles. Extracts to any folder; run from there. No installer machinery.

---

## 7. First-run setup

`synapse-mcp setup` is the wizard the operator runs once:

1. **Permissions check.** Confirm the user can write to `%LOCALAPPDATA%\synapse\`.
2. **License agreement acknowledgment.** (Per `08_anti_cheat_policy.md` §7.)
3. **ViGEmBus check.** Detect if installed; if not, offer to download and run the Nefarius installer.
4. **Model selection.** Show available detection / OCR / STT models with sizes. Operator picks which to download.
5. **Profile selection.** Show available profiles; default = enable all bundled.
6. **Bearer token generation.** For HTTP mode; store in `%APPDATA%\synapse\token.txt`.
7. **Optional hardware HID.** Detect connected RP2040 boards; offer to flash one.
8. **Start configuration.** Write `%APPDATA%\synapse\config.toml`.
9. **First server start.** Launches `synapse-mcp --mode stdio` and prompts the operator to configure their agent client.

Setup is interactive but supports `--non-interactive --accept-defaults` for headless installs.

---

## 8. config.toml schema

```toml
# %APPDATA%\synapse\config.toml

[server]
default_mode = "stdio"               # stdio | http
http_bind = "127.0.0.1:7700"
allow_non_loopback = false
tls_cert = ""
tls_key = ""

[storage]
db_path = ""                         # default: %LOCALAPPDATA%\synapse\db
nightly_compaction_hour = 3          # UTC hour
profiles_dir = ""                    # default: %APPDATA%\synapse\profiles

[retention]
# Per-CF overrides; see 07_storage_and_profiles.md §4

[logging]
level = "info"
log_dir = ""                          # default: %LOCALAPPDATA%\synapse\logs
otlp_endpoint = ""                    # empty = disabled

[metrics]
prometheus_bind = ""                  # empty = disabled, e.g., "127.0.0.1:9100"

[capture]
default_target = "primary"
min_update_interval_ms = 16

[perception]
default_mode = "auto"

[detection]
default_model = "yolov10n_general"
backend_preference = ["cuda", "directml", "cpu"]

[ocr]
default_backend = "winrt"

[audio]
loopback_enabled = true
stt_model = "whisper-tiny-int8"

[action]
default_keyboard_backend = "software"
default_mouse_backend = "software"
default_pad_backend = "vigem"
hardware_hid_port = ""                # empty = auto-detect

[safety]
panic_hotkey = "ctrl+alt+shift+p"
allow_launch = []                     # list of regexes
allow_shell = []                      # list of regexes
allow_hardware_hid_tier2 = false
no_redaction = false
require_acknowledge_on_start = true

[redaction]
[redaction.custom_patterns]
# operator-extensible
```

Schema versioned via `synapse_config_version = "1"` field at the top. Mismatch refuses to start with `CONFIG_VERSION_MISMATCH`.

---

## 9. CLI surface

`synapse-mcp` is the daemon entry; it also exposes sub-commands for operator workflows.

```
synapse-mcp [SUBCOMMAND]

Commands:
  (none)                Run as MCP server (default)
  setup                 Run first-time setup wizard
  health                Print health check and exit
  db status             Print DB summary
  db gc [--aggressive]  Run garbage collection
  db compact            Force compaction
  db wipe [--yes]       Wipe the database
  db backup <out>       Hot backup to a directory
  db restore <in>       Restore from a backup directory
  db trim --cf <name> --keep-hours <n>  Manual trim
  models list           List cached / available models
  models import <path>  Side-load a model
  models gc             Drop unreferenced models
  profiles list         List loaded profiles
  profiles install <src>  Install a profile from path or URL
  profiles validate <path>  Validate a profile file
  replay list           List replay sessions
  replay show <id>      Show session summary
  replay export <id> <out.zip>
  replay tail <id>
  metrics dump --since <duration> --output <path>
  hid identify --port <com>
  hid flash --port <com>
  token rotate
  overlay               Launch debug overlay (separate process)
  --version             Print version + build hash + signature
  --help

Top-level flags:
  --mode <stdio|http>
  --bind <addr>
  --db <path>
  --profile-dir <path>
  --log-level <level>
  --reflex-disabled
  --vigem-disabled
  --hardware-hid <port>
  --allow-launch <regex>
  --allow-shell <regex>
  --allow-hardware-hid-tier2
  --no-redaction
  --otlp-endpoint <url>
  --metrics-bind <addr>
  --no-tray
```

All flags also map to `SYNAPSE_*` env vars (e.g., `SYNAPSE_MODE=http`).

---

## 10. Logo, icons, branding

Project includes:

- `assets/logo.svg` — vector logo
- `assets/icon-256.png`, `assets/icon-32.png`, `assets/icon-16.png` — Windows icons
- `assets/installer-banner.png` — MSI installer header
- `assets/tray-icon-active.ico`, `assets/tray-icon-paused.ico`, `assets/tray-icon-error.ico`

Created at project start; design simple (a circle with a small "fork-like" symbol for "synapse"). Replaceable; no strong brand requirement.

---

## 11. Code signing

We sign:

- `synapse-mcp.exe` (the daemon)
- `synapse-overlay.exe`
- `SynapseSetup-x.y.z.msi` (the installer)
- `synapse-pico-hid.uf2` (firmware; signature embedded in metadata payload of the UF2 — not cryptographic; informational only)

Cert: an EV code-signing certificate held by the project maintainer (post-v1, when there's funding). Pre-v1: self-signed; operators see SmartScreen warning until trust is built.

Sign with `signtool.exe` from the Windows SDK:

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /a synapse-mcp.exe
```

Signing is part of the release script in `scripts/release/sign.ps1`.

---

## 12. Release process

1. **Branch:** `release/x.y.z` cut from `main`.
2. **Tag:** `vx.y.z` on the commit.
3. **CI release job** builds:
   - `synapse-mcp.exe` (release profile, signed)
   - `synapse-overlay.exe` (signed)
   - `SynapseSetup-x.y.z.msi` (signed)
   - `synapse-portable-x.y.z-windows-x64.zip`
   - `synapse-pico-hid-x.y.z.uf2`
4. **Upload to GitHub Releases** with release notes.
5. **Publish to crates.io** for `synapse-mcp` (cargo-installable).
6. **Update winget manifest** PR.

A release is signed off by the maintainer per the manual test plan in `13_testing_strategy.md` §15.

---

## 13. Reproducible builds

Goal: a given commit hash produces a byte-identical binary on any contributor's machine.

- Pin Rust toolchain via `rust-toolchain.toml`
- Use `cargo build --frozen --locked`
- Pin all dependencies in `Cargo.lock` (committed)
- Avoid build.rs that touches the network or system clock
- ONNX models referenced by sha; never bundled in the binary itself (downloaded at install or first run)

We don't yet pursue bitwise-reproducible Windows binaries (PE timestamps, COFF section ordering vary). Goal post-v1.

---

## 14. License compliance

`cargo deny check` enforces:

- Only `MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `MPL-2.0`, `ISC`, `Zlib`, `Unicode-DFS-2016` allowed
- `GPL-*`, `AGPL-*`, `SSPL-*` blocked
- Vendored deps with no SPDX identifier blocked

`THIRD-PARTY-LICENSES.md` generated by `cargo about` and included in the installer.

---

## 15. Bundled models and binary size

Default install size targets:

| Component | Size |
|---|---|
| `synapse-mcp.exe` (stripped, LTO) | ≤ 15 MB |
| `synapse-overlay.exe` | ≤ 10 MB |
| ONNX models bundled (YOLOv10n, Whisper-tiny, WinRT-OCR is OS-provided) | ≤ 80 MB |
| Profiles | ≤ 1 MB |
| Total MSI | ≤ 120 MB |

`describe` VLM is NOT bundled; downloaded on first use (~500 MB). Operator can opt out and skip `describe`.

---

## 16. Update mechanism

Synapse does not auto-update. Updates happen via:

- Running `winget upgrade Synapse.SynapseMCP`
- Downloading a new MSI manually
- `cargo install --git ... --force` for cargo installs

At startup, Synapse optionally checks GitHub Releases (off by default; `--check-updates` opt-in) for a new version and prints a one-line notice. No data sent except the User-Agent string.

---

## 17. Crate-by-crate Cargo.toml templates

Each crate's `Cargo.toml` has the same scaffold:

```toml
[package]
name = "synapse-foo"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
synapse-core = { path = "../synapse-core" }
# ... per-crate deps from [workspace.dependencies]

[dev-dependencies]
synapse-test-utils = { path = "../synapse-test-utils" }
proptest.workspace = true
insta.workspace = true
tempfile.workspace = true

[lints]
workspace = true
```

Templates committed in `scripts/new-crate.ps1`:

```powershell
.\scripts\new-crate.ps1 -Name synapse-new
```

Generates a skeleton crate following the template.

---

## 18. Documentation generation

`cargo doc --workspace --no-deps` generates API docs. We publish to docs.rs for the library crates (`synapse-core`, `synapse-storage`, etc.). The binary crate (`synapse-mcp`) is not on docs.rs but its `--help` output and this PRD are the canonical references.

---

## 19. What this doc does NOT cover

- Specific CI pipeline YAML → `.github/workflows/` in the repo
- Distribution channel publishing details → `scripts/release/`
- Firmware build details → `09_hardware_hid_gateway.md` §8
- Per-feature-flag testing combinations → `13_testing_strategy.md` §14
