# 16 — Open Questions

Things we deliberately did not decide in the PRD. Each entry has a description, the trade-off, the current default, and a decision target (which milestone or what evidence would close it).

When one of these is resolved, replace the entry with a one-line "→ decided in ADR-NNN" pointer. ADRs live in `docs/adr/NNN-title.md` (created post-v1 if needed).

---

## OQ-001 — Sled vs RocksDB as primary

**Question.** Is RocksDB the right storage backend, or should we make sled the default and RocksDB the alternate?

**Trade-off.** RocksDB has better compaction, better TTL via compaction filters, more mature on Linux. Sled is pure Rust, simpler install footprint, but slower and historically had data-loss bugs.

**Current default.** RocksDB primary; sled is an `--features sled-backend` opt-in.

**Decision target.** M3. If RocksDB shows reliability issues on Windows during M3 (more than 2 crashes traced to it across the team's testing), flip the default. Otherwise stay.

---

## OQ-002 — Streamable HTTP session vs stateless

**Question.** Should Synapse HTTP sessions be stateful (with `Mcp-Session-Id`) or stateless (per-request capability tokens)?

**Trade-off.** Stateful is simpler, matches Claude Desktop's pattern, allows reflexes/subscriptions across calls. Stateless is more scalable, harder to misuse, fits SEP-1442 direction.

**Current default.** Stateful, with `Mcp-Session-Id`. Reflexes and subscriptions are session-scoped.

**Decision target.** v2 if we ever support multi-tenant deployment; v1 stays stateful.

---

## OQ-003 — Detection model default

**Question.** YOLOv10n vs RT-DETR-s as the default detection model?

**Trade-off.** YOLOv10n is faster (~5 ms on 5090), smaller (~6 MB), but jitter is higher. RT-DETR-s is more stable across frames (~10 ms), bigger (~80 MB), better small-object recall.

**Current default.** YOLOv10n.

**Decision target.** M4. Once we have a real game profile, benchmark both on Minecraft entity detection. If RT-DETR-s materially improves agent task success, switch default.

---

## OQ-004 — Aim curve default for productivity

**Question.** Should productivity profiles default to `Instant` (snap) or `EaseInOut` (visible motion) cursor moves?

**Trade-off.** Instant is faster (~3 ms total for a click). EaseInOut feels natural, the operator sees what's happening, less likely to break flow if user is watching. Some apps respond differently to instant cursor jumps (drag detection thresholds).

**Current default.** `EaseInOut` with 80 ms travel time.

**Decision target.** M5 user feedback. If operators consistently report "feels slow," switch productivity profiles to `Instant`.

---

## OQ-005 — Reflex priority arithmetic

**Question.** When two reflexes contend for the same device, do we use a strict priority hierarchy (higher always wins) or a probabilistic mix?

**Trade-off.** Strict is predictable but can starve. Probabilistic is fair but harder to debug.

**Current default.** Strict priority, with `reflex_starved` event after 2 seconds of losing.

**Decision target.** Real-world reflex usage from M4+ feedback. Stick with strict unless starvation events become a common complaint.

---

## OQ-006 — Permission model: profile-level vs session-level

**Question.** Should permissions (`allow_launch`, `allow_shell`, `allow_hardware_hid_tier2`) be set per-profile, per-session, or globally?

**Trade-off.** Per-profile means each game/app can have its own permission posture; clean but verbose. Per-session lets the agent caller scope itself. Global is simplest.

**Current default.** Global (CLI/config) + per-profile overrides for `backends.*`. No per-session granularity.

**Decision target.** v2 if multi-session deployment becomes a thing.

---

## OQ-007 — Profile signing

**Question.** Should we sign bundled profiles and verify signatures at load? Should community profiles require a community-key signature?

**Trade-off.** Signing protects against profile tampering and adds supply-chain integrity. Adds friction for community contributors.

**Current default.** No signing at v1. Profiles are plain TOML, no code execution, low risk.

**Decision target.** Post-v1 (when there's a profile marketplace). v2 likely brings optional signing.

---

## OQ-008 — Bundled VLM for `describe`

**Question.** Should we bundle Florence-2-base (or similar) in the installer for first-run `describe` capability?

**Trade-off.** Bundled = always available, no first-use latency, +500 MB install size. Download-on-demand = smaller install, first call is slow.

**Current default.** Download-on-demand. `describe` returns `MODEL_NOT_LOADED` until operator runs `models import` or first call triggers download.

**Decision target.** v1.x. If operators consistently get confused by the first-call slow path, switch to bundled.

---

## OQ-009 — Maximum elements in `observe()` response

**Question.** What's the right `max_elements` default?

**Trade-off.** Higher = more context for the agent but more tokens. Lower = smaller responses but agent may miss important elements.

**Current default.** 60. With depth=2, this covers most focused-window contexts and stays under ~6 KB JSON.

**Decision target.** M5 telemetry. If `elements_truncated` is true in >20% of `observe()` calls, raise the default. If responses are consistently big, lower it and make agents call `expand(slot=...)` (a future tool).

---

## OQ-010 — A11y CDP integration depth

**Question.** Should we attach to every Chromium-based browser automatically or require explicit `cdp_attach`?

**Trade-off.** Auto means seamless browser perception but requires the browser to be launched with `--remote-debugging-port`. Explicit means the agent must know to attach.

**Current default.** Auto-attempt when the foreground window is a known Chromium browser AND a debugging port is configured. No silent failure — surface `CDP_UNREACHABLE` if not.

**Decision target.** M3 ship state; revise if browser users find the explicit-port requirement painful.

---

## OQ-011 — Hardware HID firmware language choice

**Question.** Rust+embassy on RP2040 vs C+TinyUSB?

**Trade-off.** Rust+embassy: consistent with the rest of the stack, type safety, but slightly larger flash footprint and slower iteration on USB stack issues. C+TinyUSB: smaller, more mature USB stack, faster to onboard external contributors.

**Current default.** Rust+embassy. We're an all-Rust project; the small overhead is worth the consistency.

**Decision target.** Locked unless USB stack bugs in embassy block M4 demo.

---

## OQ-012 — How to handle multi-monitor capture

**Question.** When the operator has multiple monitors, do we capture all of them as separate targets, or stitch into one virtual desktop?

**Trade-off.** Separate targets give cleaner CNN inputs per monitor. Stitched gives a single coherent view (e.g., a window spanning monitors).

**Current default.** Separate; the active capture target is one monitor at a time. The agent picks by `set_capture_target(monitor_index=...)`.

**Decision target.** M3. Real multi-monitor users will tell us if this is annoying.

---

## OQ-013 — Reflex `aim_track` smoothing under perception jitter

**Question.** If detection track position jitters frame-to-frame (e.g., entity at x=820 → 824 → 818 → 822), how aggressively does aim_track follow?

**Trade-off.** Following every micro-jitter = mouse hunting. Smoothing = lag.

**Current default.** A small exponential moving average (`alpha = 0.7`) applied to track position before aim error calculation. Configurable in reflex params.

**Decision target.** M4 game testing. Tune from gameplay footage.

---

## OQ-014 — Whisper-tiny vs Whisper-base for STT

**Question.** Should we default to Whisper-tiny (~40 MB, faster) or Whisper-base (~140 MB, better accuracy)?

**Trade-off.** Speed vs accuracy. STT use cases are typically for "what did the NPC say" or "what's the operator saying in voice chat" — neither is latency-critical but accuracy matters.

**Current default.** Whisper-tiny-int8. Operators wanting better can `models import whisper-base.onnx`.

**Decision target.** M5 user feedback. May add Whisper-base to the bundle if disk-size budget permits.

---

## OQ-015 — Profile match precedence

**Question.** When multiple profiles match a window (e.g., a profile matching exe and another matching title), which wins?

**Trade-off.** Order of insertion (random across machines) is bad. Most-specific match (more matched fields) is principled but expensive. User-installed > bundled is simple.

**Current default.** User-installed first, then bundled. Within a directory, alphabetical by file. First match wins.

**Decision target.** M3. If users hit ambiguity often, switch to "most-specific" scoring.

---

## OQ-016 — Action coalescing

**Question.** If the agent rapidly fires `MouseMoveRelative(1, 0)` 100 times in quick succession, should the emitter coalesce them into one `MouseMoveRelative(100, 0)`?

**Trade-off.** Coalescing reduces USB poll pressure on hardware HID. Not coalescing preserves exact timing.

**Current default.** No coalescing for software backend (cheap anyway). Coalescing for hardware backend when target poll interval would be missed (deferred ≤ 2ms of pending small moves merge).

**Decision target.** M4 hardware HID testing. Tune coalescing window.

---

## OQ-017 — Disk pressure level thresholds

**Question.** Are 2 GB / 1 GB / 500 MB / 200 MB the right disk-free thresholds for pressure levels 1-4?

**Trade-off.** Higher = wastes free disk for users with small SSDs. Lower = risk running out.

**Current default.** Listed thresholds. Operator can override via config.

**Decision target.** v1.x telemetry from operators on small drives.

---

## OQ-018 — Replay export format

**Question.** ZIP with JSONL + frames vs SQLite db vs custom binary format?

**Trade-off.** ZIP is portable, inspectable. SQLite is queryable. Custom binary is compact but opaque.

**Current default.** ZIP+JSONL+WebP frames.

**Decision target.** Locked unless replay viewer (v2) needs a different format for performance.

---

## OQ-019 — How to expose telemetry for debugging vs production

**Question.** Should debug telemetry (per-event traces) be available via the same `/metrics` endpoint that ops uses, or split?

**Trade-off.** Single endpoint = simpler. Split = production ops don't get spammed with debug-level data.

**Current default.** Single endpoint, but `metrics_level` config (`production` | `debug`) controls verbosity. Default `production`.

**Decision target.** M5. May split if operators complain about endpoint size.

---

## OQ-020 — Should we expose raw frame access at all?

**Question.** The agent could ask for a raw screenshot via `act_screenshot_once`. This violates our "structure over pixels" principle but is sometimes needed (e.g., for `describe` fallback). Do we expose it or force everything through `describe`?

**Trade-off.** Exposing raw screenshots is the escape hatch agents need. Hiding them forces structure but may break workflows.

**Current default.** Expose `game_screenshot_once` (renamed from `act_screenshot_once` for clarity) as an escape hatch. Document it as "use sparingly."

**Decision target.** M3. Make sure it doesn't become the default path; M5 telemetry should show <5% of agent turns calling it.

---

## OQ-021 — Audio direction estimate vs full spatial

**Question.** Stick with naive L/R energy + cross-correlation, or invest in HRTF-based estimation via Steam Audio?

**Trade-off.** Naive is cheap, accurate to ~30° azimuth. Steam Audio is precise but pulls a big dep.

**Current default.** Naive at v1. Steam Audio is v2.

**Decision target.** v1.x if audio-direction-based reflexes prove popular and accuracy is a complaint.

---

## OQ-022 — Reflex `on_event` reactive recursion guard

**Question.** Can an `on_event` reflex's action emit an event that triggers another `on_event` reflex? If so, what limits prevent infinite loops?

**Trade-off.** Allowing chained reflexes is powerful. Without guards it's a footgun.

**Current default.** Allowed. Per-tick guard: max 4 reflex firings per tick across all reflexes. Exceeded → emit `REFLEX_RECURSION_LIMIT` event and skip remaining firings until next tick.

**Decision target.** M3 implementation; revisit if guard turns out to be insufficient.

---

## OQ-023 — How "stable" is element_id across UIA snapshots

**Question.** UIA's `RuntimeId` is documented as stable for a given element across the lifetime of the element, but in practice can change after structural mutations. Do we trust it, or maintain our own ID layer?

**Trade-off.** Trusting saves work. Maintaining a layer is more reliable but more code.

**Current default.** Composite ID (`<hwnd>:<runtime_id_hex>`) is our public identifier. We re-resolve on every action call via the runtime ID; if that fails, we fall back to looking up by name+role+position. The agent may need to re-`observe()` if elements churn.

**Decision target.** M2 testing. If stability issues surface, build a wrapper.

---

## OQ-024 — Tokenization budget enforcement

**Question.** Should `observe()` proactively trim to stay under a token budget (e.g., 1500 tokens), or return everything and let the agent's client trim?

**Trade-off.** Server-side trim is more reliable; client trim is more flexible.

**Current default.** Server-side trim with explicit `Observation.diagnostics.elements_truncated` flag so the agent knows.

**Decision target.** M3. May add `max_tokens` param to `observe()` for fine control.

---

## OQ-025 — Bundled detection model license

**Question.** YOLOv8/YOLOv10/YOLOv11 are AGPL-licensed (Ultralytics). Can we bundle weights?

**Trade-off.** AGPL is incompatible with MIT/Apache. Bundling is forbidden. Operator-downloaded weights are the operator's problem.

**Current default.** **DO NOT BUNDLE Ultralytics-trained weights.** We provide model loader infrastructure; operator downloads weights themselves. We bundle alternatively licensed models (CC0 / Apache) when available — e.g., RT-DETR-s with its Apache-2.0-friendly trained checkpoints.

**Decision target.** Locked. We track post-v1 alternatives that we CAN bundle (license permitting).

---

## OQ-026 — Cross-platform when

**Question.** Linux and macOS land in v2. What's the trigger to start?

**Trade-off.** Cross-platform doubles platform surface but expands user base.

**Current default.** Start Linux work when v1.0 is shipped and we have 1000+ stars or paying interest from a partner.

**Decision target.** Post-v1.

---

## OQ-027 — Operator multi-factor for Tier 2 hardware HID

**Question.** Should enabling hardware-HID against Tier 2 games require a second-factor confirmation (e.g., physical button press on the Pico) instead of just CLI flags?

**Trade-off.** Higher friction = harder to abuse but harder to use legitimately.

**Current default.** CLI flag + interactive prompt + per-call profile flag (three gates) is sufficient.

**Decision target.** v2 if abuse is observed.

---

## OQ-028 — Schema versioning policy

**Question.** Pre-v1 we wipe DB on schema change. Post-v1 do we support migrations or stay wipe-and-rebuild?

**Trade-off.** Migrations are operator-friendly. Wipe is simpler to develop.

**Current default.** Wipe at v1.0 (with a strong release note). Migrations land in v1.1 if real users need them.

**Decision target.** v1.1.

---

## OQ-029 — Notifications channel discipline

**Question.** When pushing 100 events/second to a subscribing client, do we send one notification per event, or batch?

**Trade-off.** Per-event = lower latency, more chatty. Batched = fewer round trips, latency jitter.

**Current default.** Per-event, with the agent's client doing its own batching if needed. Synapse never delays delivery.

**Decision target.** M3 perf testing. If notification throughput is a bottleneck, batch in 10 ms windows.

---

## OQ-030 — How aggressive default GC

**Question.** Does the GC task run every 5 minutes, or wake on cap-exceeded?

**Trade-off.** Periodic = consistent. Reactive = lower idle cost.

**Current default.** Periodic 5 min + reactive on writes that exceed soft cap.

**Decision target.** M5 telemetry. Tune cadence based on observed CF growth patterns.

---

## 2. How to use this list

Anyone reading the PRD who finds an unresolved decision should:

1. Check this doc first — likely it's already here.
2. If not, add an entry. Format:

```
## OQ-NNN — <one-line summary>

**Question.** ...

**Trade-off.** ...

**Current default.** ...

**Decision target.** <milestone> or <condition>
```

3. Bump the next OQ number; don't reuse.

When a question is decided, replace the body with:

```
## OQ-NNN — <summary> — DECIDED <date>

→ See ADR-NNN.
```

Keep this file lean. It is a parking lot for honest uncertainty; not a TODO list.

---

## 3. What this doc does NOT cover

- Resolved decisions (those move to ADRs)
- Specific implementation TODOs (those live in code comments and issue tracker)
- Bug list (issue tracker)
- Feature requests (issue tracker / discussions)
