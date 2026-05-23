# 13 — Testing Strategy

## 1. Why testing is hard here

Synapse is hard to test because:

- It touches the real OS (UIA trees, real windows, real input emission)
- It runs at frame rate (test setups that take 5 seconds to fixture aren't useful)
- Many bugs are timing-sensitive (a reflex that fires correctly when CPU is idle but races a slow capture under load)
- The output domain (HID input) is observable only by another OS process, not by the test
- Some components (hardware HID) require physical hardware to exercise fully

So we layer the tests carefully. Unit tests are cheap and ubiquitous. Integration tests are scoped to subsystems with fakes for the OS layer. End-to-end tests run against real Windows but in CI as opt-in jobs on self-hosted runners.

We never let "but it's hard to test" become an excuse for not testing.

---

## 2. The test pyramid

| Layer | Count | Where | Per-PR? |
|---|---|---|---|
| **Unit** | 1000s | Inside each crate (`#[cfg(test)] mod tests`) | Yes |
| **Integration** | 100s | Workspace-level `tests/integration/` | Yes |
| **Property-based** | 10s of properties | `proptest` in critical crates | Yes |
| **Snapshot** | 10s | `insta` crate for stable outputs | Yes |
| **Performance regression** | dozens of benches | `criterion`-based benches | Weekly CI |
| **End-to-end on real Windows** | ~10 scenarios | `tests/e2e/` driven by `synapse-mcp` | Nightly self-hosted |
| **Hardware-in-the-loop** | ~5 scenarios | RP2040 attached to runner | Weekly self-hosted |
| **Profile validation** | Per profile | Auto-generated from `profiles/*.toml` | Yes |

---

## 3. Unit tests

Standard Rust unit tests. Rules:

- Every public function with non-trivial logic has at least one unit test
- Every error variant has a test that triggers it
- Every `pub const` (CF names, error codes) is asserted to match its literal in a test
- No `unwrap()` outside test code
- Tests with non-deterministic input use a fixed seed

Example contract test:

```rust
#[test]
fn cf_names_match_constants() {
    assert_eq!(synapse_core::cf::CF_EVENTS, "events");
    assert_eq!(synapse_core::cf::CF_OBSERVATIONS, "observations");
    // ... 1 per CF
}

#[test]
fn error_codes_match_constants() {
    use synapse_core::error_codes::*;
    assert_eq!(ACTION_QUEUE_FULL, "ACTION_QUEUE_FULL");
    assert_eq!(OBSERVE_NO_PERCEPTION_AVAILABLE, "OBSERVE_NO_PERCEPTION_AVAILABLE");
    // ... 1 per code
}
```

Drift between code and these docs becomes a test failure.

---

## 4. Integration tests

Scoped to a subsystem with the OS layer replaced by a fake. Layered like real production but composable.

### 4.1 Capture fakes

`synapse-capture` exposes a `MockCaptureSource` that emits a sequence of fixture frames. Tests for perception, detection, OCR run against this without touching the GPU.

```rust
let source = MockCaptureSource::from_dir("tests/fixtures/frames/menu_screen/")?;
let perception = Perception::new(source, /* ... */);
let observation = perception.observe()?;
assert_eq!(observation.entities.len(), 3);
```

### 4.2 UIA fakes

`synapse-a11y` exposes a `MockUiaTree` for deterministic tests. The mock implements the same `UIElement`-like interface but reads from a JSON fixture:

```json
{
  "root": {
    "name": "Untitled - Notepad",
    "role": "Window",
    "children": [
      {"name": "File", "role": "MenuItem", "patterns": ["Invoke"]},
      ...
    ]
  }
}
```

Real UIA is tested in E2E tests only.

### 4.3 Action sinks

`synapse-action::backends::software::Backend` is `SendInput`-based in production; in tests we substitute a `RecordingBackend` that records all calls without emitting OS input.

Reflex runtime tests use this exclusively — they verify "given this event stream, this sequence of actions would have been emitted" without ever touching the real OS.

### 4.4 Storage isolation

Every test gets a `tempfile::TempDir`-backed RocksDB instance via `synapse-test-utils::TestDb`. Wipe-on-drop.

---

## 5. Property-based tests

`proptest` crate. Critical areas where invariants matter:

- **Event filter evaluator** — round-trip serialization, ordering of `And` / `Or` doesn't change result, `Not(Not(x)) == x` for total filters.
- **Aim curves** — generated start/end points produce step sequences whose first step starts at start, last step ends at end, total ms matches duration within tolerance.
- **Keystroke dynamics** — generated text round-trips correctly, no chars dropped, modifier-state consistent.
- **Coordinate transforms** — `screen_to_window(window_to_screen(p, h), h) == p` for any window.
- **Bincode round-trip** — every persistable type round-trips bytes-identical.

Critical bug class: an action emitter that drops a `KeyUp` after a `KeyDown` is a stuck-key. Property test:

```rust
proptest! {
    #[test]
    fn no_stuck_keys(actions in vec(arb_action(), 0..100)) {
        let mut emitter = ActionEmitter::new(RecordingBackend::new());
        for action in &actions {
            emitter.execute(action.clone()).unwrap();
        }
        emitter.flush();
        emitter.release_all();
        assert!(emitter.backend().held_keys().is_empty());
        assert!(emitter.backend().held_buttons().is_empty());
    }
}
```

---

## 6. Snapshot tests

`insta` crate. For stable outputs (tool schemas, observation JSON shape, error response shape):

```rust
#[test]
fn observation_schema_snapshot() {
    let obs = sample_observation();
    insta::assert_json_snapshot!(obs);
}
```

If the schema changes, `cargo insta review` lets the developer accept the new snapshot. Reviewers see the diff in the PR — schema changes are visible.

---

## 7. Performance regression tests

`criterion` benches for hot paths:

```rust
fn bench_observe_warm(c: &mut Criterion) {
    let setup = warm_synapse();
    c.bench_function("observe_warm", |b| {
        b.iter(|| setup.observe(default_params()))
    });
}
```

Run weekly in CI on a known-hardware self-hosted runner. Results stored in `bench_results/<commit_sha>/`. PR perf-CI compares head vs main, flags any >20% regression on a tracked benchmark.

Tracked benches:

| Bench | Target p99 |
|---|---|
| `observe_warm_a11y_only` | ≤ 10 ms |
| `observe_warm_hybrid` | ≤ 30 ms |
| `event_to_subscriber` | ≤ 50 ms |
| `reflex_tick_jitter_idle` | ≤ 200 µs |
| `reflex_tick_jitter_under_load` | ≤ 500 µs |
| `aim_curve_step_calc_natural` | ≤ 1 µs |
| `action_software_press` | ≤ 3 ms |
| `detection_yolov10n_640` | ≤ 8 ms (with GPU) |
| `ocr_winrt_120x32` | ≤ 8 ms |
| `serialize_observation_typical` | ≤ 5 ms |

---

## 8. End-to-end tests (real Windows)

Run on a self-hosted Windows 11 runner. CI tag: `windows-e2e`. Each test:

1. Spawns `synapse-mcp` in stdio mode
2. Connects as an MCP client (custom Rust client in `synapse-test-utils`)
3. Drives a scripted scenario
4. Asserts observed events + outcomes

Test scenarios:

| Scenario | Verifies |
|---|---|
| `notepad_type_save` | Open Notepad, type text, save file, verify file content |
| `vscode_open_file` | Open VS Code with file argument, observe element tree, find file in explorer |
| `chrome_navigate_form_submit` | Open Chrome, navigate URL, fill form, submit, observe response |
| `terminal_run_command` | Open Windows Terminal, type command, observe output |
| `multiwindow_focus_switch` | Open three apps, cycle focus, verify foreground events |
| `clipboard_round_trip` | Write clipboard from agent, read in different app, verify |
| `reflex_aim_track_static` | Track a stationary detected entity, verify aim_track stays on target |
| `reflex_combo_frame_perfect` | Execute a 3-step combo, verify HID emission times |
| `safety_release_all_on_panic` | Hold keys, kill daemon, verify all keys released |
| `disk_pressure_response` | Fill DB to soft cap, verify GC runs and cleanup happens |

Each scenario takes 5–60 seconds. Total nightly run: ~15 minutes.

### 8.1 Determinism

E2E tests are scripted but the OS isn't deterministic. We make them resilient by:

- Waiting on UIA events rather than `sleep`
- Using `find()` to locate elements rather than coordinates
- Asserting on event sequences (loose ordering with constraints) rather than exact timestamps

A test that times out waiting for an event fails with the last event log so the operator can see why.

### 8.2 Headless game scenarios

Game E2E is harder — we don't want to require GPU + game on the runner. Approach:

- Use `cargo-mommy`-style fake-game test rig: a small Bevy app that renders predictable scenes and emits known windows/HUD layouts.
- E2E tests target this fake game first; real-game tests are manual demo scripts the maintainer runs at release time.

---

## 9. Hardware-in-the-loop tests

Self-hosted runner has an RP2040 board attached. Test rig:

- Pico flashed with Synapse firmware
- A second Pico configured as a **measurement device** that captures HID reports and timestamps them

Test scenarios:

| Scenario | Asserts |
|---|---|
| `hid_mouse_move_latency` | Round-trip latency p99 ≤ 5 ms |
| `hid_combo_timing` | 3-step combo step intervals within 0.5 ms of scheduled |
| `hid_release_all_on_disconnect` | When host disconnects, watchdog releases everything within 1 s |
| `hid_high_volume` | 10,000 mouse-move commands at full rate, no drops |
| `hid_reflash` | Reset to bootloader, flash, verify new identity |

Run weekly. Hardware tests are flagged optional in CI; they pass-through if the runner doesn't have hardware attached, but log a warning.

---

## 10. Profile validation tests

Every `profiles/*.toml` is auto-validated on PR:

- Parses against `Profile` struct
- All keymap aliases resolve to known key codes
- All HUD region pointers are in-bounds
- All `event_extensions` have valid `EventFilter` syntax
- All model_ids resolve to a known model

A PR that breaks a profile fails before merge.

Additionally, each bundled profile has a small smoke test scenario:

```rust
#[test]
fn profile_minecraft_smoke() {
    let prof = synapse_profiles::load("minecraft.java").unwrap();
    assert_eq!(prof.mode, PerceptionMode::PixelOnly);
    assert!(prof.hud.iter().any(|h| h.name == "hp_hearts"));
    assert!(prof.keymap.contains_key("attack"));
}
```

---

## 11. Fuzz testing

`cargo-fuzz` harnesses for protocol parsers:

- MCP JSON-RPC parser
- HID serial protocol frame parser
- EventFilter parser
- Profile TOML parser

Each fuzz target runs nightly with `--max-total-time=600` (10 minutes). Crashes are CI-blocking; corpus is committed.

---

## 12. Soak tests

`tests/soak/` directory. Long-running test that:

- Spawns Synapse
- Runs a synthetic workload (frame source emits at 60 fps, fake agent calls `observe()` at 2 Hz, reflexes register/cancel at 0.1 Hz)
- Runs for 8 hours
- Asserts at end:
  - Memory growth ≤ 50 MB over the run
  - No deadlocks
  - p99 latencies stable across the run (no drift)
  - DB size respects soft caps

Triggered manually or weekly on a dedicated runner.

---

## 13. Replay-driven regression tests

Captured replay sessions become regression test fixtures. Workflow:

1. Operator hits a bug
2. Operator exports the replay (`synapse-mcp replay export`)
3. Bug filed with the `.zip`
4. Maintainer adds the replay as a fixture in `tests/replays/<bug_id>/`
5. A test loads the fixture, feeds Synapse the events, asserts the bug is fixed

This builds a regression corpus from real bugs over time.

---

## 14. CI matrix

GitHub Actions (or equivalent):

| Job | OS | Trigger |
|---|---|---|
| `cargo fmt --check` | ubuntu | every PR |
| `cargo clippy --workspace --all-targets -- -D warnings` | windows | every PR |
| `cargo test --workspace` | windows | every PR |
| `cargo test --workspace --no-default-features` | windows | every PR |
| `cargo build --release --workspace` | windows | every PR |
| `cargo deny check` | ubuntu | every PR |
| `cargo audit` | ubuntu | every PR + daily cron |
| `insta review --check` | ubuntu | every PR |
| `e2e-real-windows` | self-hosted windows | nightly |
| `bench-regression` | self-hosted windows | weekly |
| `hardware-in-loop` | self-hosted with Pico | weekly |
| `soak` | self-hosted windows | weekly |
| `fuzz` | ubuntu | nightly, 10min per target |

Self-hosted runners are documented in `scripts/runners/` so a contributor can set up their own and run the same pipeline.

---

## 15. Manual test plan (release gate)

Before tagging a release, the maintainer runs a manual test plan:

1. **Fresh install on a clean Windows 11 VM.** Install ViGEmBus, install Synapse, connect Claude Desktop, run "open Notepad, type, save" scenario.
2. **Live game session.** Pick one bundled game profile, play for 15 minutes via the agent, verify reasonable behavior and no stuck inputs.
3. **Hardware HID flash + smoke.** Flash a Pico, connect, run hardware aim test.
4. **Panic hotkey drill.** Start a long-running reflex, hit `Ctrl+Alt+Shift+P`, verify everything stops within 100 ms.
5. **Disk pressure drill.** Fill a small DB volume, verify pressure transitions, verify operation continues degraded but not broken.

The maintainer signs off with a release-notes entry summarizing what they tested.

---

## 16. Code coverage targets

- `synapse-core`: 95% line coverage. Pure types + small logic; must be exhaustive.
- `synapse-storage`, `synapse-profiles`, `synapse-reflex`, `synapse-action`: 85%
- `synapse-capture`, `synapse-a11y`, `synapse-audio`, `synapse-perception`: 70% (OS-bound code, harder to cover)
- `synapse-models`, `synapse-hid-host`, `synapse-telemetry`: 80%

`tarpaulin` for coverage measurement on Linux (where possible) + Windows for OS-bound crates. CI surfaces coverage delta on each PR; >5% drop blocks merge.

---

## 17. What this doc does NOT cover

- Specific test fixture details → fixtures live in `tests/fixtures/`
- CI configuration files → `.github/workflows/`
- Hardware test rig wiring → `09_hardware_hid_gateway.md`
- Profile authoring tutorial → community wiki (post-v1)
