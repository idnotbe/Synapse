# 12 — Observability

## 1. What "observable" means here

Synapse is a real-time system with real-time failure modes. We need three things from observability:

1. **Live diagnosis.** When something feels wrong (slow, missed click, stuck reflex), the operator can immediately see which subsystem is degraded and why.
2. **Post-hoc replay.** When the agent fails at a task, the operator (or the developer) can step through what Synapse saw and what it did.
3. **Trend detection.** Performance regressions, growing memory, growing CFs, increasing dropped frames — caught before they become outages.

We build all three from the same primitives: `tracing` spans, structured events, metrics histograms, and the replay log. Nothing fancy.

---

## 2. Telemetry primitives

### 2.1 Tracing (`tracing` crate)

Every subsystem instruments with `#[tracing::instrument]` or manual `span!` macros. Spans nest naturally. Each span carries:

- Span name (kebab-case, e.g., `capture.frame`, `perception.detect`, `action.emit`)
- Subsystem tag
- Relevant IDs (`session_id`, `reflex_id`, `seq`)
- Latency on close

Levels: `error` `warn` `info` `debug` `trace`. Defaults to `info`. Operator can change via `--log-level debug` or `RUST_LOG=synapse_capture=debug`.

### 2.2 Metrics (`metrics` crate + a small wrapper)

Three metric types:

- **Counter:** monotonically increasing values (`frames_dropped_total`)
- **Histogram:** latency distributions (`mcp_tool_latency_seconds{tool}`)
- **Gauge:** point-in-time values (`cf_size_bytes{cf}`, `active_reflexes`)

All registered through `synapse-telemetry::metrics`. Names are lowercase snake_case, `_total` suffix for counters, `_seconds` for time, `_bytes` for size.

### 2.3 Events (the replay log)

Already covered in `06_data_schemas.md` and `07_storage_and_profiles.md`. Events are the canonical record of "what happened" — perception, actions, reflexes, all flow through the event bus and into `CF_EVENTS`.

---

## 3. Logging

### 3.1 Backends

Two `tracing-subscriber` layers active in production:

| Layer | Purpose | Format |
|---|---|---|
| Console layer (stderr) | Operator-visible during foreground runs | Pretty (`tracing-subscriber::fmt`) when stderr is a TTY; JSON otherwise |
| File layer | Persistent record | JSON Lines |

Operator can enable an OTLP exporter (`opentelemetry-otlp`) by setting `--otlp-endpoint http://localhost:4317`. Off by default.

### 3.2 File location and rotation

```
%LOCALAPPDATA%\synapse\logs\
├── synapse.log               # current, JSONL
├── synapse.log.2026-05-21.gz # daily rotation, oldest first to delete
├── synapse.log.2026-05-20.gz
├── ...
└── capture/
    ├── capture.log
    └── capture.log.2026-05-21.gz
```

Rotation policy (enforced by `tracing-appender::rolling::Builder`):

- Daily rollover at local midnight
- Keep 7 daily rotated files
- Compress rotated files to gzip
- Total log directory cap: 500 MB (oldest pruned first)
- Per-file size cap: 500 MB (rotate when reached even mid-day)

Compression and pruning run in a low-priority background tokio task on a 1-hour timer.

### 3.3 What's logged at which level

| Level | Examples |
|---|---|
| `error` | Storage corruption, capture FFI failure, panic-hook fires, hardware HID disconnect mid-action |
| `warn` | Backpressure dropping events, model load fallback (DirectML → CPU), profile detection ambiguity, disk pressure level entered |
| `info` | Daemon start/stop, session open/close, profile activation, model loaded, capture target changed, reflex registered/cancelled |
| `debug` | Per-tool request summary, per-event-kind counts, per-frame latencies |
| `trace` | Raw event payloads (redacted), action emission details, COM call results |

Default level is `info` and that's what 90% of operators ever see. `debug` and `trace` are for issue reproduction.

### 3.4 Structured fields

Every log line carries:

```json
{
  "timestamp": "2026-05-22T15:00:00.123Z",
  "level": "info",
  "target": "synapse_perception::detect",
  "fields": {
    "subsystem": "perception",
    "session_id": "...",
    "frame_seq": 12345,
    "detection_count": 7,
    "latency_ms": 4.2,
    "message": "detection complete"
  },
  "span": {"name": "perception.detect", "id": 42}
}
```

Spans embedded so the consumer can reconstruct the call tree without correlating timestamps.

### 3.5 Redaction passes through logging

Same `synapse-core::redact` patterns from `11_security_and_safety.md` apply. The console + file + OTLP layers run the redactor on free-form string fields before emission. This is non-negotiable; even `trace` level redacts unless `--no-redaction` is set.

---

## 4. Metrics

### 4.1 Core metric set

Subsystem health:

```
synapse_uptime_seconds (gauge)
synapse_subsystem_status{subsystem,status} (gauge 0/1)
synapse_panics_total (counter)
```

Capture:

```
capture_frames_total{target}
capture_frames_dropped_total{target,reason}
capture_frame_interval_seconds{target} (histogram)
capture_texture_pool_size_bytes
```

Perception:

```
perception_observations_total{mode}
perception_observation_assemble_seconds (histogram)
perception_observation_size_bytes (histogram)
detection_inferences_total{model_id}
detection_inference_seconds{model_id} (histogram)
detection_detections_per_frame (histogram)
ocr_calls_total{backend}
ocr_call_seconds{backend} (histogram)
ocr_cache_hits_total
a11y_events_total{kind}
a11y_snapshot_seconds (histogram)
cdp_connections_active
```

Audio:

```
audio_loopback_frames_total
audio_events_total{kind}
audio_transcriptions_total
audio_transcription_seconds (histogram)
```

Action:

```
action_emitted_total{backend,kind}
action_failed_total{backend,kind,error}
action_emit_seconds{backend} (histogram)
action_queue_depth (gauge)
action_queue_full_total
held_inputs_active (gauge)
```

Reflex:

```
reflex_registered_active (gauge)
reflex_fired_total{kind}
reflex_cancelled_total{reason}
reflex_tick_jitter_seconds (histogram)
reflex_tick_late_total
reflex_starved_total
```

MCP:

```
mcp_sessions_active (gauge)
mcp_requests_total{tool}
mcp_requests_failed_total{tool,error}
mcp_request_seconds{tool} (histogram)
mcp_push_notifications_total{kind}
mcp_subscriptions_active (gauge)
```

Storage:

```
cf_size_bytes{cf} (gauge)
cf_rows{cf} (gauge)
storage_writes_total{cf}
storage_batch_flush_seconds (histogram)
cache_hits_total{cf}
cache_misses_total{cf}
cache_evictions_total{cf,reason}
storage_disk_pressure_level (gauge)
storage_disk_free_bytes (gauge)
```

Hardware HID (when attached):

```
hid_frames_sent_total
hid_frames_acked_total
hid_frames_naked_total{reason}
hid_link_timeouts_total
hid_watchdog_fires_total
hid_round_trip_seconds (histogram)
```

### 4.2 Metric labels

Labels are bounded — never put unbounded values (session IDs, reflex IDs, image hashes) in a label. They explode cardinality.

Allowed labels: subsystem name, error code (from the closed set in `06_data_schemas.md` §8), CF name (closed set), backend, model_id (closed set, ~5 values), tool name (closed set, 30 tools).

### 4.3 Exposition

Three exposition mechanisms:

| Mechanism | When | Format |
|---|---|---|
| `health` MCP tool | Agent or operator calls | JSON |
| `/metrics` HTTP endpoint | `--metrics-bind <addr>` set | Prometheus text format |
| OTLP push | `--otlp-endpoint <url>` set | OTLP protobuf over gRPC |

The Prometheus endpoint is the most common operator path. Hook into Grafana/Mimir and you have charts.

### 4.4 Local ringbuffer fallback

If no OTLP / Prometheus endpoint is configured, last 6 hours of metric samples are kept in `CF_TELEMETRY`. Operator can query via:

```
synapse-mcp metrics dump --since 1h --output csv > metrics.csv
```

This is the "I noticed something weird, let me look at the last hour" workflow without needing external infra.

---

## 5. The `health` MCP tool (operator-and-agent view)

Already documented in `05_mcp_tool_surface.md` §3.29. Repeating the response shape for completeness:

```json
{
  "ok": true,
  "subsystems": {
    "capture": {"status": "healthy", "fps": 60, "frames_dropped_60s": 0},
    "a11y": {"status": "healthy", "events_60s": 412},
    "audio": {"status": "healthy", "device": "Speakers (Realtek...)"},
    "perception": {"status": "healthy", "detection_p99_ms": 4.2, "ocr_p99_ms": 7.8},
    "action": {"status": "healthy", "queue_depth": 0, "held_inputs": 0},
    "reflex": {"status": "healthy", "active_count": 2, "tick_jitter_us_p99": 180},
    "storage": {"status": "healthy", "db_size_mb": 234, "disk_pressure": 0},
    "hid": {"status": "disconnected"},
    "models": {"loaded": ["yolov10n", "whisper-tiny"]}
  },
  "retention": {
    "cf_events": {"ttl_hours": 24, "live_mb": 842, "soft_cap_mb": 2048},
    "...": "..."
  },
  "version": "0.1.0",
  "build": "abc123",
  "uptime_s": 1245
}
```

Subsystem `status` values match `06_data_schemas.md::SensorStatus`.

Agents poll this when they sense something off. Operators use it as the "everything OK?" dashboard.

---

## 6. Debug overlay

Optional in-process overlay window that renders telemetry over a transparent always-on-top window. Launched via:

```
synapse-mcp overlay
```

Shows:

- Real-time frame rate, detection p99, action queue depth
- Active reflexes (each with name, fired count, last fired)
- Recent events (rolling list)
- Hot keys: `Ctrl+Alt+Shift+L` toggle, `Ctrl+Alt+Shift+P` panic (same as main)
- Disk pressure level + DB size

Built with `egui` + `eframe`. Standalone binary; not part of `synapse-mcp` proper, but in the same workspace (`crates/synapse-overlay/`).

The overlay is read-only — observes telemetry, doesn't emit actions. Operator-facing UX.

---

## 7. Replay tooling

`synapse-mcp replay` CLI:

```bash
synapse-mcp replay list                     # list sessions
synapse-mcp replay show <session_id>        # summary of a session
synapse-mcp replay export <session_id> out.zip
synapse-mcp replay tail <session_id>        # follow live session
synapse-mcp replay search "act_click"       # search by tool/event kind/text
```

`replay show` outputs a JSONL transcript of events + actions + observations interleaved by time. Pipeable into `jq` for filtering.

`replay export` produces a self-contained `.zip` (Synapse Web Replay format) with:

- `manifest.json` (session id, time range, Synapse version, agent client)
- `events.jsonl` (full event log for the session)
- `actions.jsonl` (full action log)
- `observations/{seq}.json` (each persisted observation snapshot)
- `frames/{seq}.webp` (if `--include-frames` and the session had bookmarked frames)

The `.zip` is plain — no encryption — and includes redaction applied per `11_security_and_safety.md` §5.

### 7.1 Replay viewer (future, not v1)

A web-based replay viewer reads the `.zip` and shows a timeline with events, actions, and observations. Not built at v1; planned for v2 when there's a real reason to invest in UI.

---

## 8. Tracing in production

In production runs (long-running daemon), tracing goes to file + (optionally) OTLP. `info` level is verbose enough to diagnose 90% of issues from logs alone:

- Every session open/close → log line
- Every profile activation → log line
- Every action emission → log line (one per action, batched if rate > 100/s)
- Every reflex registration → log line
- Every disk pressure transition → log line
- Every model load → log line

`debug` doubles the volume; `trace` is for development.

Log volume at info on a typical 1-hour gameplay session: ~50 MB uncompressed JSONL. After gzip rotation: ~5 MB.

---

## 9. Crash dumps

A panic hook in `synapse-telemetry::panic_handler`:

1. Logs the panic message + backtrace at `error`
2. Writes a crash dump to `%LOCALAPPDATA%\synapse\crashes\YYYYMMDD-HHMMSS.dump` with the panic, version, build hash, last 100 log lines, and last 100 events
3. Fires `release_all` via the static handle (see `03_action.md` §11)
4. Re-panics to abort the process

Crash dumps retained for 30 days. Operator can attach to a bug report.

---

## 10. Performance profiling integration

Standard `tracing-flame` + `pprof` available behind feature flags:

```
cargo run --features perf-profiling -- --mode stdio
# Generates flamegraph.svg on Ctrl+C
```

Not enabled by default; ~5% overhead when active. CI runs a perf-profiling job on a weekly basis to produce flamegraphs of the standard test scenarios.

---

## 11. Specific dashboards (operator templates)

Bundled Grafana dashboards JSON in `dashboards/` directory:

- `synapse_overview.json` — high-level health + uptime + sessions
- `synapse_perception.json` — capture FPS, detection latency, OCR cache hit rate, a11y event rate
- `synapse_action.json` — action latency by backend + kind, queue depth, error rate
- `synapse_storage.json` — CF sizes, disk pressure, cache hit rates, GC frequency
- `synapse_reflex.json` — active reflex count, tick jitter, fired counts by kind

Operators import these into Grafana once. Dashboards drift slowly; we update them with major Synapse releases.

---

## 12. What to look at when something is wrong (operator playbook)

**Symptom: actions feel laggy.**
- Check `action_emit_seconds` p99 by backend
- Check `action_queue_depth` (high = saturation)
- Check `reflex_tick_jitter_seconds` (spikes = host overload)

**Symptom: `observe()` is slow.**
- Check `perception_observation_assemble_seconds` p99
- Check `a11y_snapshot_seconds` p99 (high = UIA cross-process slowdown)
- Check `detection_inference_seconds` p99 (high = GPU contention)

**Symptom: events feel stale.**
- Check `event_to_subscriber_latency_seconds`
- Check event bus drops via `events_dropped_for_subscriber{}` counter

**Symptom: DB growing.**
- `synapse-mcp db status`
- Look at `cf_size_bytes{cf}` to find the offender
- Check disk pressure level + last GC time

**Symptom: hardware HID drops out.**
- Check `hid_link_timeouts_total`
- Check `hid_frames_naked_total{reason}`
- Restart by reconnecting USB; auto-reconnect should kick in

**Symptom: a reflex misfires.**
- `reflex_history --reflex-id <id>` to see fires + filter matches
- Check `reflex_starved_total{reflex_id}`
- Look at `CF_REFLEX_AUDIT` directly via `ldb`

Every "symptom" has a metric. If you can't find one, file an issue — that's a doc bug.

---

## 13. What this doc does NOT cover

- Per-tool metric details → in `05_mcp_tool_surface.md` and `10_performance_budget.md`
- Storage retention → `07_storage_and_profiles.md` §6
- Replay format details → `07_storage_and_profiles.md` §5 + this doc §7
- Specific Grafana dashboard JSON → `dashboards/` directory in the repo
