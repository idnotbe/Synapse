# 15 — Roadmap and Milestones

## 1. Milestone overview

Six milestones from empty repo to production-ready:

| Milestone | Theme | Effort estimate (solo, focused) |
|---|---|---|
| **M0** | Bootstrap — workspace, MCP loopback, CI | 1 week |
| **M1** | Perception MVP — capture + UIA + observe() | 2-3 weeks |
| **M2** | Action MVP — kbd/mouse/pad + clipboard | 2 weeks |
| **M3** | Reflex + MCP surface — full tool set, push events, profiles | 2-3 weeks |
| **M4** | Hardware HID + first game profile | 2-3 weeks |
| **M5** | Production polish — installer, docs, multiple profiles, perf | 3-4 weeks |

**Total: ~14 weeks (3.5 months) solo full-time** to a shippable v1.0. Two engineers in parallel: ~8 weeks.

Each milestone has a hard demo criterion. If the demo doesn't pass, the milestone isn't done. Bug-fixing and polish stretch the timeline but never collapse the demo.

---

## 2. M0 — Bootstrap (1 week)

**Goal:** empty repo to "MCP server that returns hardcoded data."

### Scope

- Cargo workspace scaffold with 15 crates (most empty stubs)
- `synapse-core` types (the minimum: `Backend`, `Point`, `Rect`, error codes for the M0 path)
- `synapse-mcp` binary with `rmcp` integration
- One tool registered: `health` returning a hardcoded `{"ok": true, "version": "..."}`
- stdio transport working with Claude Desktop / Codex CLI
- `tracing` JSON file logger
- CI: `cargo fmt`, `cargo clippy`, `cargo test` on every PR
- README with "Hello, Synapse" instructions
- `synapse-test-utils` with a custom MCP client for tests

### Out of scope

- Any real perception or action
- Storage (use `()` placeholder if needed)
- Profiles
- Models

### Demo criterion

Open Claude Desktop, configure Synapse as an MCP server, ask Claude to call the `health` tool, see `{"ok": true}` in Claude's response.

### Files created

```
Cargo.toml, rust-toolchain.toml, deny.toml, .gitignore
LICENSE-MIT, LICENSE-APACHE
README.md
docs/                                  (this PRD)
crates/synapse-core/
crates/synapse-mcp/
crates/synapse-test-utils/
crates/synapse-storage/                (stub)
crates/synapse-perception/             (stub)
crates/synapse-action/                 (stub)
crates/synapse-reflex/                 (stub)
crates/synapse-capture/                (stub)
crates/synapse-a11y/                   (stub)
crates/synapse-audio/                  (stub)
crates/synapse-profiles/               (stub)
crates/synapse-hid-host/               (stub)
crates/synapse-models/                 (stub)
crates/synapse-telemetry/
crates/synapse-overlay/                (stub)
.github/workflows/ci.yml
scripts/release/ (skeleton)
```

---

## 3. M1 — Perception MVP (2-3 weeks)

**Goal:** Synapse can describe any focused window as structured JSON.

### Scope

- `synapse-capture`: integrate `windows-capture` crate; emit `CapturedFrame` over crossbeam channel
- `synapse-a11y`:
  - UIA tree walker with depth-limited snapshot
  - WinEvent hook (foreground change, focus change, value change, structure change)
  - One small UIA cache (focused element)
- `synapse-perception`:
  - Stub detection (returns empty unless model loaded)
  - WinRT OCR wrapper
  - `Observation` assembler
- `synapse-models`: minimum ONNX loader; YOLOv10n loadable via `ort`
- `synapse-mcp` adds tools: `observe`, `find`, `read_text`, `set_capture_target`, `set_perception_mode`, `health`
- Coordinate transforms (per-monitor DPI awareness)

### Out of scope

- Audio
- HUD profiles
- Reflexes
- Action
- Replay log

### Demo criterion

Open Notepad with the cursor in the editor. Agent calls `observe()` and gets a JSON response with `foreground.process_name = "notepad.exe"`, `focused.role = "Edit"`, and the editor's bounding rect. Total round trip ≤ 50 ms.

### Risk areas

- UIA cross-process COM marshaling can be slow; need cache request batching working from day one
- DirectX texture lifetime is unforgiving; expect to spend time on `Drop`/`RAII` correctness
- `ort` + DirectML setup on a clean Windows install has paperwork (install MSVC redist, etc.)

---

## 4. M2 — Action MVP (2 weeks)

**Goal:** Synapse can drive any app's input.

### Scope

- `synapse-action`:
  - Software backend via `enigo` + direct `windows-rs` where needed
  - Action serialization actor (single mpsc emitter)
  - `ReleaseAll` safety on shutdown / panic
  - Held-input tracking + auto-release timeout
  - ViGEm backend via `vigem-client`
  - Coordinate transforms for screen/window/element click resolution
  - UIA `InvokePattern` semantic invoke for elements that support it
- `synapse-mcp` adds tools: `act_click`, `act_type`, `act_press`, `act_aim`, `act_drag`, `act_scroll`, `act_pad`, `act_clipboard`, `release_all`
- Aim curves: `Instant`, `Linear`, `EaseInOut`, `Bezier`, `Natural`
- Keystroke dynamics: `Burst`, `Linear`, `Natural`

### Out of scope

- Hardware HID
- Combos (deferred to M3 with reflex runtime)
- Run-shell / launch

### Demo criterion

Agent calls `act_click(element_id=<Notepad editor>)`, `act_type(text="Hello")`, `act_press(keys=["ctrl","s"])`, and the save dialog appears. Subsequent `observe()` shows the dialog.

### Risk areas

- ViGEm requires the user to install ViGEmBus; CI runner needs it preinstalled
- `Natural` curve takes design+test cycles to feel right; default everywhere is `EaseInOut` until M5

---

## 5. M3 — Reflex + MCP Surface (2-3 weeks)

**Goal:** Synapse supports push-event subscriptions, registered reflexes, profiles, and the full tool surface.

### Scope

- `synapse-reflex`:
  - Event bus (`crossbeam` broadcast pattern)
  - Reflex scheduler on dedicated time-critical thread
  - All five reflex kinds: `aim_track`, `hold_move`, `hold_button`, `combo`, `on_event`
  - Audit log to `CF_REFLEX_AUDIT`
- `synapse-storage`:
  - RocksDB integration
  - CF set from `07_storage_and_profiles.md`
  - Compaction filters for TTL
  - GC task with soft/hard caps
  - Disk pressure responder
- `synapse-profiles`:
  - TOML loader with full schema
  - Hot-reload via `notify` crate
  - Detection logic (exe + title match)
  - Bundled profiles: `notepad`, `vscode`, `chrome`, `terminal`
- `synapse-mcp` adds: `subscribe`, `subscribe_cancel`, `reflex_register`, `reflex_cancel`, `reflex_list`, `reflex_history`, `profile_list`, `profile_activate`, `replay_record`, `audio_tail`, `audio_transcribe`
- Streamable HTTP transport (in addition to stdio)
- Push notifications via SSE for subscriptions
- `synapse-audio` MVP: WASAPI loopback + simple direction estimate + Whisper-tiny STT

### Out of scope

- Hardware HID
- Game profiles
- Debug overlay
- VLM-based `describe`

### Demo criterion

Agent registers `on_event` reflex: "when a Save dialog appears, type a path and press Enter." Triggers the save flow in Notepad by `act_press(["ctrl","s"])`; the reflex fires automatically; no further agent intervention until the file is saved.

### Risk areas

- Time-critical thread scheduling on Windows is fiddly; expect to debug jitter
- Hot-reload of profiles vs. active state needs careful ordering
- Streamable HTTP / SSE re-connect semantics are subtle
- RocksDB on Windows has had reliability issues; the sled-backend escape hatch matters

---

## 6. M4 — Hardware HID + First Game Profile (2-3 weeks)

**Goal:** Synapse can play one game end-to-end, including via hardware HID.

### Scope

- `synapse-hid-host`:
  - Serial driver
  - Identity handshake
  - Frame protocol with CRC, ACK/NAK, watchdog
  - Reconnect logic
- `firmware/pico-hid/`:
  - RP2040 firmware in Rust with `embassy-rp`
  - USB HID composite (mouse + keyboard + pad) + CDC ACM serial
  - Watchdog
  - Protocol parser
  - LED status feedback
  - `.uf2` build pipeline
- `synapse-mcp` adds: `act_combo` (now using the reflex scheduler), `act_run_shell` (gated), `act_launch` (gated), `hid identify`, `hid flash`
- First game profile: `minecraft.java`
  - HUD: hp_hearts, hunger, xp
  - Keymap with Minecraft-default bindings
  - Detection model: YOLOv10n_general (no Minecraft-specific fine-tuning yet)
  - `event_extensions`: `creeper_nearby`, `low_hp`
- Anti-cheat policy enforcement (`08_anti_cheat_policy.md`): profile tier flagging, backend gating

### Out of scope

- Multiple game profiles (Minecraft is the lighthouse; others land in M5)
- VLM-based `describe`
- Debug overlay
- Installer

### Demo criterion

Agent connects to Synapse with Minecraft running. It calls `observe()`, sees the player's HP and visible entities. It walks the agent through "find a tree and break it" → "make planks" → "make a workbench" via `act_press`, `act_aim`, and a couple of registered reflexes for `auto_attack_low_hp`. Demo runs for 5 minutes without intervention.

Bonus: same demo via hardware HID (set `--hardware-hid auto`).

### Risk areas

- Game detection model accuracy on Minecraft (a small specialty fine-tune may be required)
- HUD OCR for hearts/hunger is template-match; needs assets carefully cropped
- Hardware HID latency under sustained load; benchmark + tune

---

## 7. M5 — Production Polish (3-4 weeks)

**Goal:** v1.0 ship-ready.

### Scope

- Installer (`SynapseSetup.msi`) via `wix-installer`
- Code signing (self-signed at first; project cert when funded)
- 5+ additional bundled profiles:
  - `factorio`
  - `discord` / `slack`
  - `file_explorer`
  - `<one_fps>` (TBD; probably a free game)
  - `roblox_studio`
- Debug overlay (`synapse-overlay`)
- VLM-based `describe` (Florence-2-base ONNX)
- Full Grafana dashboards
- Complete docs (this PRD + a user-facing `USER_GUIDE.md` distinct from the PRD)
- Stable schema (v1 schema locked, future changes go through migration / DB wipe)
- All performance budgets in `10_performance_budget.md` met on a reference machine
- Soak test passing 8 hours clean
- Crash dump infrastructure
- `synapse-mcp setup` wizard
- Tray icon
- License + token management
- Public release on GitHub Releases + crates.io
- winget submission

### Demo criterion

Fresh Windows 11 machine, no Synapse pre-installed. Operator runs `synapse setup`, follows the wizard, then opens Claude Desktop and the agent successfully completes:

1. Open VS Code and write a small Rust file
2. Run `cargo build` via terminal
3. Switch to Chrome, search for "Synapse MCP project," read a result
4. Switch to Minecraft, play for 2 minutes
5. Switch to a music player, control playback

All without screenshots, with token cost under 30K total for the whole sequence.

---

## 8. Post-v1 — what comes after

v1 ships at M5. Major v2+ work, prioritized:

### v1.x patches

- Per-game fine-tuned detection models (`yolov10n_minecraft`, `yolov10n_factorio`, etc.)
- Improvements to `Natural` aim curve based on user feedback
- More bundled profiles via community contributions

### v2 horizons

| Feature | Effort |
|---|---|
| **Linux support (Wayland + AT-SPI)** | ~6 weeks |
| **macOS support (AX + ScreenCaptureKit + native input)** | ~6 weeks |
| **Cross-platform CDP** (already half-cross-platform via `chromiumoxide`) | ~1 week |
| **Per-game RAM hooks for sanctioned games (Minecraft via mod API, KSP via plugin)** | ~2 weeks per game |
| **Visual replay viewer (web app)** | ~4 weeks |
| **Profile marketplace** (community-contributed profiles with signing) | ~4 weeks |
| **Steam Audio integration for spatial audio** (replace naive direction with HRTF) | ~2 weeks |
| **Sub-millisecond aim via PIO USB host on RP2040** (pass-through real mouse + corrections) | ~3 weeks |
| **Browser DOM-only mode** (Synapse as a structured-DOM RPA backend; no a11y, no pixels) | ~2 weeks |

None of these are committed; the v2 roadmap is decided after v1 ships and we see what real users want.

---

## 9. Risks and mitigations (per milestone)

| Milestone | Risk | Mitigation |
|---|---|---|
| M0 | rmcp API churn | Pin to specific rmcp version; track via cargo dependency PRs only after vetting |
| M1 | UIA performance worse than expected | Cache request batching from day one; fall back to depth-1 snapshots if slow |
| M1 | DirectML availability on AMD/Intel | CPU fallback for detection; warn at startup if no GPU EP |
| M2 | ViGEm install friction | Document installer step prominently; auto-detect at startup; skip ViGEm-backed features if not installed |
| M3 | Time-critical thread jitter on Windows | Use multimedia timer; document; fall back to `tokio::time` with 2 ms tick if MMCSS unavailable |
| M3 | RocksDB Windows hiccups | sled-backend feature flag as escape valve |
| M4 | RP2040 firmware bugs frustrating to debug | Loopback build feature for off-target testing; CI on a self-hosted Pico |
| M4 | Minecraft detection accuracy | Mark accuracy lower than docs claim; commit to a fine-tune in v1.x |
| M5 | MSI signing cert availability | Self-sign at v1.0; SmartScreen warning documented; community cert acquisition is a separate workstream |
| M5 | VLM bundle size | VLM is optional download, not bundled; `describe` returns `MODEL_NOT_LOADED` until downloaded |

---

## 10. Acceptance criteria summary

A release is shippable when all of these are true:

1. All milestones M0–M5 demos pass.
2. Performance budgets in `10_performance_budget.md` met on the reference machine (RTX 3060 + 8-core CPU).
3. CI green for 3 consecutive days on `main` (no flakes from intermittent test failures).
4. Soak test passes 8 hours.
5. Manual test plan in `13_testing_strategy.md` §15 signed off.
6. PRD docs in this directory are internally consistent (no broken cross-references).
7. License compliance clean (`cargo deny check`).
8. No `unsafe` outside the documented allowed crates.
9. No `unwrap()` outside test code (`#[deny(clippy::unwrap_used)]`).
10. Crash dumps verified to land on intentional panics.

---

## 11. Out-of-bound items (not scheduled at v1)

- AI-driven profile authoring (have the agent generate profiles)
- Cloud-hosted Synapse-as-a-service
- Multi-machine orchestration
- Mobile (iOS / Android) MCP clients driving Synapse remotely
- Sandbox / VM auto-provisioning for safety
- Encrypted replay exports
- Real-time co-pilot mode (agent + human sharing input)

Each of these is a fine v2+ idea but cleanly outside v1 scope.

---

## 12. The v1 promise

When v1 ships, the operator gets:

- A signed Windows installer that puts Synapse on their PATH
- A first-run wizard that takes ≤ 5 minutes
- An MCP server compatible with every major agent client
- ≤ 30 ms p99 `observe()` for productivity apps
- Real-time game support for at least 2 single-player titles
- A documented hardware HID path for accessibility / research
- A complete PRD + user guide + reference docs
- An active community on GitHub Issues + Discussions
- A roadmap of what's coming in v1.x patches

That's the contract.

---

## 13. What this doc does NOT cover

- Specific issue tracker / project board → GitHub Projects, not in docs
- Sprint planning / iteration cadence → maintainer's workflow, not a project artifact
- Commercial roadmap (if any) → out of scope
- Specific demo-game choices → finalized closer to M4 launch
