# 07 — Storage and Profiles

## 1. Storage philosophy

Synapse is a single-machine, single-tenant service. The storage layer exists for three reasons:

1. **Replay debugging.** Every event, every action, every reflex firing is persisted so a session can be replayed deterministically when something breaks.
2. **Caches.** OCR results, downloaded models, profile loads — anything expensive to compute or fetch lives in RocksDB after the first access.
3. **Session continuity.** A session-id keyed view that lets a re-connecting client resume subscriptions and rediscover its registered reflexes.

We deliberately do **not** persist:

- Captured frames (huge; only replay log keeps small diff hashes)
- Audio waveforms (huge; only summarized event metadata)
- Raw UIA tree snapshots beyond the 1-Hz `CF_OBSERVATIONS` sample
- The agent's MCP request payloads beyond minimal trace metadata

Storage is **wipe-friendly**. Schema changes = wipe and rebuild. No migrations. Pre-v1; once v1 ships, schema changes require ADR + tooling.

---

## 2. Backend choice: RocksDB (with sled fallback)

RocksDB via the `rocksdb` crate. Pinned version with explicit feature flags (`features = ["multi-threaded-cf"]`).

Why RocksDB:

- Mature on Windows (despite some warts; `bzip2`/`zlib` C dep is unavoidable but stable)
- Column families let us scope reads/writes precisely
- TTL / compaction filters give us cheap rolling retention
- Snapshot reads for the replay tool

Why a sled fallback:

- RocksDB has had Windows reliability hiccups in the past
- Some operator environments forbid native C deps
- `sled` is pure Rust, simpler footprint, slower but adequate

Feature flag `synapse-storage/sled-backend` swaps the implementation. Same API surface; both implement `trait Db`.

Default: RocksDB. Sled is `--feature sled-backend` opt-in for v1; promoted to first-class if RocksDB causes issues in production.

---

## 3. Database location

```
default:    %LOCALAPPDATA%\synapse\db\
override:   --db <path>     (CLI flag)
            SYNAPSE_DB_PATH (env)
            config.toml: db.path
```

Subdirectory layout:

```
%LOCALAPPDATA%\synapse\
├── db\                            # RocksDB instance
├── models\                        # ONNX model cache (separate from CF_MODEL_CACHE for very large files)
├── profiles\                      # User-installed profiles override bundled
├── replay\                        # Replay session exports (manual)
├── logs\                          # tracing logs
└── config.toml                    # operator-level config
```

---

## 4. Column families

| CF | Key | Value | Encoding | TTL | Soft cap | Hard cap | Notes |
|---|---|---|---|---|---|---|---|
| `CF_EVENTS` | `[seq u64 BE]` | `StoredEvent` | bincode | 24h | 2 GB | 4 GB | Append-only ring; the replay log |
| `CF_OBSERVATIONS` | `[seq u64 BE]` | `StoredObservation` | bincode | 6h | 500 MB | 1 GB | 1Hz sample + reason-triggered snapshots |
| `CF_PROFILES` | `[profile_id utf8]` | TOML bytes | raw bytes | none | 20 MB | 50 MB | Cached load; source of truth is on-disk TOML |
| `CF_MODEL_CACHE` | `[model_sha256 32 bytes]` | model bytes | raw bytes | LRU when full | 1 GB | 2 GB | Downloaded ONNX models, sha-verified |
| `CF_SESSIONS` | `[session_id utf8]` | `StoredSession` | json | 30d | 50 MB | 100 MB | One row per session |
| `CF_REFLEX_AUDIT` | `[reflex_id 16 bytes][at_ns u64 BE]` | `StoredReflexAudit` | bincode | 7d | 200 MB | 500 MB | Per-reflex audit |
| `CF_OCR_CACHE` | `[image_sha256 32 bytes]` | `OcrResult` | bincode | 1h | 50 MB | 100 MB | Memoization of OCR on stable regions |
| `CF_TELEMETRY` | `[metric_name utf8][at_ns u64 BE]` | `f64 LE` | raw 8 bytes | 6h | 100 MB | 200 MB | Local metric ringbuffer |
| `CF_ACTION_LOG` | `[at_ns u64 BE][seq u32 BE]` | `StoredActionRecord` | bincode | 24h | 200 MB | 500 MB | Every action emitted |
| `CF_PROCESS_HISTORY` | `[at_ns u64 BE][pid u32]` | json | json | 6h | 20 MB | 50 MB | Process started/exited events |
| `CF_KV` | `[utf8]` | bytes | raw | none | 10 MB | 50 MB | Generic key-value extension |

**Defaults are conservative.** Default total DB size budget is **~4 GB** including write-amplification. Soft cap = start aggressive expiry. Hard cap = refuse writes for that CF, surface `STORAGE_CF_HARD_CAP_REACHED`.

All retention values are operator-configurable in `config.toml`:

```toml
[retention.cf_events]
ttl_hours = 24
soft_cap_mb = 2048
hard_cap_mb = 4096

[retention.cf_ocr_cache]
ttl_hours = 1
soft_cap_mb = 50
hard_cap_mb = 100
```

Caps and TTLs default to research-friendly values. Operators running on small disks should lower them; operators doing forensic debugging may raise them.

`pub const`s for each CF name live in `synapse-core::cf`. Constants match the on-disk strings exactly. A test asserts the match — drift fails CI.

### 4.1 Key encoding rules

- Time-keyed CFs (`CF_EVENTS`, `CF_OBSERVATIONS`, `CF_REFLEX_AUDIT`, `CF_ACTION_LOG`, `CF_TELEMETRY`, `CF_PROCESS_HISTORY`): `u64` big-endian for natural sort order on `time-ascending` iteration. Add a `seq` or `pid` suffix as needed for uniqueness.
- ID-keyed CFs: UTF-8 strings (UUIDs as canonical hex). Easier to inspect with `ldb`.
- Hash-keyed CFs: raw 32-byte sha256.

### 4.2 TTL implementation

Per-CF compaction filter that drops records older than the CF's TTL. Implemented in `synapse-storage::compaction`. Runs on every compaction; effective expiry within ~1 hour of nominal TTL.

Compaction filters consult the runtime config so TTL changes take effect on the next compaction without a restart. The filter is a small `Box<dyn CompactionFilter>` per CF; it decodes only the timestamp portion of each value (not the full record) for speed.

The active retention policy is exported through the `health` MCP tool and the Prometheus endpoint so the operator can see what's actually being enforced vs. what they configured.

### 4.3 Write batches

All writes go through `Db::write_batch(Batch)` to minimize fsync cost. A `Batch` accumulates puts/deletes across multiple CFs. The storage task batches writes from a `mpsc::Receiver<WriteOp>` channel; flush triggers:

- Every 100 ms (idle batch flush)
- Every 64 KB accumulated
- Every explicit `Db::flush()` call (used at session close, after `act_run_shell`, etc.)

---

## 5. Replay log semantics

`CF_EVENTS` is the canonical replay source. Combined with `CF_OBSERVATIONS` it reconstructs any past 24h of session activity.

Replay tool: `synapse-mcp replay --session <id> [--speed 1.0] [--out <dir>]`:

1. Reads all `CF_SESSIONS` row for the id.
2. Reads `CF_EVENTS` and `CF_ACTION_LOG` for the session's time range.
3. Reads matching `CF_OBSERVATIONS` snapshots.
4. Produces a JSONL transcript + a Synapse Web Replay (SWR) bundle.
5. Optional `--simulate-actions` flag replays actions against the live machine (useful for repro).

SWR bundle is a single `.zip` containing the JSONL transcript + extracted observation snapshots + the active profile at the time. Self-contained; can be shipped for bug reports.

---

## 6. Data lifecycle and cleanup (the contract)

This is the binding policy for what gets persisted, for how long, and how it gets removed. The agent rarely cares about data older than a few minutes; the AI's working memory lives in its context, not in Synapse's DB. Synapse stores only what's useful for **debugging, replay, caching, and a short rolling history**.

### 6.1 Data classes

Every byte in the system falls into one of four classes:

| Class | Storage | Retention | Example |
|---|---|---|---|
| **Ephemeral hot** | RAM only | Until consumed or dropped | Captured frames, audio ring buffer, event-bus backlog, in-flight detection results |
| **Short-term durable** | RocksDB with aggressive TTL | hours to days | `CF_EVENTS` (24h), `CF_ACTION_LOG` (24h), `CF_OBSERVATIONS` (6h), `CF_TELEMETRY` (6h) |
| **Cache** | RocksDB with LRU + TTL | until evicted | `CF_OCR_CACHE` (1h), `CF_MODEL_CACHE` (LRU 1GB), `CF_PROFILES` (none, tiny) |
| **Audit / long-lived** | RocksDB, longer TTL | days to weeks | `CF_SESSIONS` (30d), `CF_REFLEX_AUDIT` (7d) |

**Nothing is persisted forever.** Every CF either has a TTL or is a bounded cache. The two non-TTL CFs (`CF_PROFILES`, `CF_KV`) are tiny by design and never grow above a few MB.

### 6.2 Why these retentions

| CF | Retention | Why |
|---|---|---|
| `CF_EVENTS` | 24h | Replay debugging mostly works on "what happened in the last session." Anything older is forensic and should be exported. |
| `CF_OBSERVATIONS` | 6h | 1 Hz samples × 6h = 21,600 snapshots. Enough to reconstruct any recent session. |
| `CF_ACTION_LOG` | 24h | Same as events. |
| `CF_REFLEX_AUDIT` | 7d | Reflex debugging crosses sessions; longer view helps spot patterns. |
| `CF_SESSIONS` | 30d | Session metadata is small; useful for long-term usage analysis. |
| `CF_OCR_CACHE` | 1h | OCR results are recomputable. Short cache; high churn region of the disk. |
| `CF_MODEL_CACHE` | LRU only | Models don't expire by age; they expire by disuse. |
| `CF_TELEMETRY` | 6h | Local fallback metrics. If the operator wants long-term retention, push to OTLP/Prometheus instead. |
| `CF_PROCESS_HISTORY` | 6h | Useful only for "what was running an hour ago when X happened." Older is noise. |

### 6.3 Three layers of cleanup

Synapse runs three independent cleanup mechanisms. None of them block the hot path.

**Layer 1 — RocksDB compaction filters.** Per-CF compaction filter drops expired rows during the natural compaction process. No separate scan. This is the cheapest path and handles ~95% of expirations.

**Layer 2 — Periodic GC task.** A dedicated `storage_gc` tokio task runs every 5 minutes:

1. For each CF with a soft cap, query `db.property_int_value("rocksdb.estimate-live-data-size")`.
2. If estimated size > soft cap, run `DeleteRange` against the oldest 25% of keys for that CF.
3. After deletion, request a compaction over that key range so disk is reclaimed promptly.
4. Update `cache_evictions_total{cf}` and `cf_size_bytes{cf}` metrics.

The GC task uses bounded work per tick — no single tick takes more than 100 ms of CPU. If the soft cap is grossly exceeded (>2× soft), it runs in tighter intervals (1 min) until back under.

**Layer 3 — Disk-pressure responder.** A separate `storage_disk_pressure` task wakes whenever free disk on the DB volume drops below 2 GB:

1. **Level 1 (free < 2 GB):** Tighten all TTLs to 50% of nominal (e.g., events 24h → 12h). Emit `STORAGE_DISK_PRESSURE_LEVEL_1` event.
2. **Level 2 (free < 1 GB):** Drop the cache CFs entirely (`CF_OCR_CACHE`, `CF_TELEMETRY`, `CF_PROCESS_HISTORY`). Re-tighten TTLs to 25% nominal. `STORAGE_DISK_PRESSURE_LEVEL_2`.
3. **Level 3 (free < 500 MB):** Halt new writes to all non-essential CFs (telemetry, OCR cache, model cache for new downloads). Surface `STORAGE_WRITE_FAILED` for those CFs. `STORAGE_DISK_PRESSURE_LEVEL_3`.
4. **Level 4 (free < 200 MB):** Refuse new MCP sessions. Existing sessions get a one-line warning event. Action emission continues (don't strand held inputs).

State machine transitions are debounced — switching levels requires the free-space change to persist for 30 seconds.

### 6.4 Session-end cleanup

When an MCP session ends (clean close, transport drop, or process shutdown), Synapse performs immediate cleanup:

1. Cancel all reflexes registered by that session (each cancellation logged to `CF_REFLEX_AUDIT`).
2. Close all open subscriptions for that session.
3. Emit `release_all` for any inputs held by that session's reflexes.
4. Write a `closed_at` timestamp to the session's `CF_SESSIONS` row.
5. Schedule `CF_OBSERVATIONS` snapshots taken during this session for short-half-life cleanup (most observations expire 6h after capture; session-end-marked observations get a 2h half-life since they're more likely to be of interest soon).

No per-session DB-wipe is needed. The TTL mechanism handles cleanup naturally.

### 6.5 Application-level retention overrides

The agent can mark certain data for shorter or longer retention via tool parameters:

```
observe(include=["focused","elements"], retain_hint="ephemeral")     # do not write CF_OBSERVATIONS row
observe(include=["focused","elements"], retain_hint="bookmark")      # extend this snapshot to 30d retention
```

Bookmarks are useful when the agent realizes "this moment was important; keep it for debugging." Default `retain_hint` is `"standard"` (the CF's TTL).

Bookmarked observations live in a sub-prefix of `CF_OBSERVATIONS` (`bookmark:` prefix) with its own 30-day TTL. There's a cap of 100 bookmarks per session to prevent abuse.

### 6.6 Cache management

| Cache | Eviction trigger | Size cap |
|---|---|---|
| `CF_OCR_CACHE` | TTL 1h + LRU when > 50 MB | 50 MB |
| `CF_MODEL_CACHE` | LRU when > 1 GB; ONNX never used in last 30d gets evicted first | 1 GB |
| `CF_PROFILES` | None (tiny) | Few MB |
| `CF_TELEMETRY` | TTL 6h + LRU when > 200 MB | 200 MB hard cap |

LRU bookkeeping piggybacks on the row's `last_read_at_ns` timestamp updated on read. Updates are batched per 10 reads to avoid write amplification.

Cache eviction runs in the storage GC task. Tracked via:

- `cache_evictions_total{cf, reason}` (`reason ∈ {ttl, lru, soft_cap, disk_pressure}`)
- `cache_hit_ratio{cf}` (rolling 1h window)
- `cf_size_bytes{cf}`

### 6.7 RocksDB-level space reclamation

Deleting a key in RocksDB only writes a tombstone. Disk isn't reclaimed until compaction. Synapse triggers compaction explicitly after aggressive deletions:

- After GC layer-2 work on a CF
- After `DeleteRange` operations
- After disk-pressure level-2 cleanup
- On `synapse-mcp db compact` operator command

A scheduled background compaction also runs nightly (configurable: `[storage] nightly_compaction_hour = 3`) to keep disk usage predictable.

### 6.8 Operator-facing visibility

The operator can always inspect storage state:

```bash
$ synapse-mcp db status
DB path:      C:\Users\alice\AppData\Local\synapse\db
Total size:   1.42 GB on disk (live: 1.05 GB, garbage: 0.37 GB)
Disk free:    87.4 GB on volume C:

Column family       Live MB   TTL    Soft  Hard   Status
CF_EVENTS             842.1   24h   2048  4096   OK
CF_OBSERVATIONS       104.5   6h     500  1000   OK
CF_ACTION_LOG          18.2   24h    200   500   OK
CF_REFLEX_AUDIT         3.1   7d     200   500   OK
CF_SESSIONS             0.4   30d     50   100   OK
CF_OCR_CACHE           47.8   1h      50   100   warning (95% of soft)
CF_TELEMETRY           38.9   6h     100   200   OK
CF_MODEL_CACHE        612.3   LRU   1024  2048   OK
CF_PROCESS_HISTORY      0.3   6h      20    50   OK
CF_PROFILES             0.1   none    20    50   OK
CF_KV                   0.0   none    10    50   OK

Pressure level:  0 (healthy)
Last compaction: 2026-05-22 03:00 UTC (4h 22m ago)
Last GC:         2026-05-22 07:18 UTC (3m 42s ago)
```

And take action:

```bash
$ synapse-mcp db gc --aggressive       # immediate full GC pass
$ synapse-mcp db compact               # force compaction
$ synapse-mcp db trim --cf CF_EVENTS --keep-hours 6
$ synapse-mcp db wipe --yes            # nuclear option
```

### 6.9 What is NEVER persisted

The list of things we never write to disk regardless of TTL:

- **Captured frame pixels.** Only metadata + dirty-region SHA hashes for replay matching.
- **Audio waveforms.** Only event metadata + summarized direction estimates.
- **Full UIA snapshots beyond the 1 Hz observation samples.** Live tree state stays in RAM.
- **Raw model intermediate tensors.** Only inference results.
- **Free-form clipboard content beyond a 120-char redacted excerpt.**
- **HTTP/WS transport message bodies.** Only trace metadata.

These are RAM-only. They live as long as the immediate consumer needs them.

### 6.10 Log file rotation

`tracing` JSON logs in `%LOCALAPPDATA%\synapse\logs\synapse.log` rotate via `tracing-appender::rolling`:

- Daily rotation
- Keep last 7 days
- After rotation, compress to `.log.gz`
- Total log directory cap: 500 MB; oldest beyond that pruned

Logs at `info` level. Per-subsystem logs (`logs/capture.log`, `logs/perception.log`) follow the same policy.

### 6.11 Replay export retention

`%LOCALAPPDATA%\synapse\replay\` holds exported SWR bundles created by the operator via `synapse-mcp replay export`. Synapse never writes here automatically. The folder is operator-managed; we surface its size in `db status` for visibility but never auto-delete.

If the folder exceeds 5 GB, `db status` flags it with a warning and suggests cleanup.

### 6.12 Best practices summary

For developers extending Synapse:

1. **Default to ephemeral.** RAM-only unless there's an explicit replay/debug reason to persist.
2. **Pick a TTL upfront.** Every new CF must declare TTL + soft cap + hard cap in `synapse-core::retention::DEFAULTS`. No "we'll figure it out later."
3. **Don't write per-frame.** Aggregate. 60 fps means 5,184,000 events per day if you write per frame. Batch.
4. **Use write batches.** Many small writes → one batch flush every 100 ms.
5. **Use bincode for hot CFs, JSON for human-readable / audit CFs.** Bincode is 2-3× smaller and 5-10× faster to decode.
6. **Don't store what you can recompute cheaply.** Detection results, OCR results, panel materializations — recompute is often cheaper than retaining.
7. **If you need long-term retention, push to external storage** (OTLP for metrics, replay export for events). Don't grow internal CFs unbounded.
8. **Compaction filter, then GC task, then disk pressure.** Layer your cleanup. Don't rely on a single mechanism.
9. **Surface CF size in `health` and Prometheus.** Operators need to see what's growing.
10. **Test with a low disk volume.** CI runs a scenario with a 1 GB tmpfs DB target to verify pressure-level responses fire correctly.

---

## 7. Cache management (legacy section header — see §6.6)

(Content moved to §6.6 as part of the unified data lifecycle policy.)

---

## 7. Operator-visible operations

| Command | Effect |
|---|---|
| `synapse-mcp db status` | DB path, total size, per-CF row count + bytes |
| `synapse-mcp db wipe` | Drops everything; confirms `--yes` flag |
| `synapse-mcp db backup <out>` | Hot backup via RocksDB checkpoint API |
| `synapse-mcp db restore <in>` | Stops daemon, restores from backup, restarts |
| `synapse-mcp db compact` | Force compaction across all CFs |
| `synapse-mcp models list` | Inventory of cached models |
| `synapse-mcp models import <path>` | Side-load a model file |
| `synapse-mcp models gc` | Drop unreferenced models |

---

## 8. Profile system

Profiles drive per-app and per-game behavior. Synapse ships a handful of bundled profiles; the operator can write more.

### 8.1 Profile directories (precedence high → low)

1. **`--profile-dir <path>`** (CLI override)
2. **`%APPDATA%\synapse\profiles\`** (user-installed)
3. **`profiles/`** beside the executable (bundled)

Within a directory, files are `<id>.toml`. ID is the basename without extension and matches the `id` field in the file. Mismatch → `PROFILE_PARSE_ERROR`.

### 8.2 TOML schema

Concrete TOML mapped to the `Profile` Rust struct from `06_data_schemas.md`. Example below for a productivity app:

```toml
# profiles/vscode.toml
id = "vscode"
label = "Visual Studio Code"
version = "1.0.0"

[[matches]]
exe = "Code.exe"

[[matches]]
exe = "VSCode.exe"

mode = "a11y_only"

[capture]
target = { kind = "foreground_window" }
min_update_interval_ms = 100
cursor_visible = true

[detection]
model_id = "none"             # disable detection for this app
classes_of_interest = []
confidence_threshold = 0.0
max_detections = 0

[ocr]
default_backend = "winrt"

# no HUD fields for VS Code
hud = []

[keymap]
save = "ctrl+s"
quick_open = "ctrl+p"
command_palette = "ctrl+shift+p"

[backends]
default = "software"
keyboard_default = "software"
mouse_default = "software"
pad_default = "vigem"
```

Game profile example:

```toml
# profiles/minecraft.java.toml
id = "minecraft.java"
label = "Minecraft Java Edition"
version = "1.0.0"

[[matches]]
exe = "javaw.exe"
title_regex = "Minecraft\\* [0-9]"

mode = "pixel_only"

[capture]
target = { kind = "foreground_window" }
min_update_interval_ms = 16
cursor_visible = true

[detection]
model_id = "yolov10n_general"
classes_of_interest = ["player", "zombie", "skeleton", "creeper", "villager"]
confidence_threshold = 0.45
max_detections = 32

[ocr]
default_backend = "winrt"

[[hud]]
name = "hp_hearts"
extractor = { kind = "template_match", templates = ["hearts/full.png", "hearts/half.png", "hearts/empty.png"] }
parser = { kind = "number" }
region = { kind = "anchored_to_edge", edge = "bottom_left", x_offset = 220, y_offset = -50, w = 180, h = 18 }

[[hud]]
name = "hunger"
extractor = { kind = "template_match", templates = ["hunger/full.png", "hunger/half.png", "hunger/empty.png"] }
parser = { kind = "number" }
region = { kind = "anchored_to_edge", edge = "bottom_right", x_offset = -400, y_offset = -50, w = 180, h = 18 }

[keymap]
forward = "w"
back = "s"
left = "a"
right = "d"
jump = "space"
sneak = "shift"
sprint = "ctrl"
attack = "lmb"
place = "rmb"
inventory = "e"
drop = "q"
chat = "t"
hotbar1 = "1"
hotbar2 = "2"
hotbar3 = "3"

[backends]
default = "software"
keyboard_default = "software"
mouse_default = "software"
pad_default = "vigem"

[[event_extensions]]
name = "creeper_nearby"
from_filter = { op = "and", args = [
    { op = "kind", kind = "entity-appeared" },
    { op = "data", path = "/class_label", predicate = { op = "eq", value = "creeper" } },
    { op = "data", path = "/bbox/w", predicate = { op = "gt", value = 80 } }
] }
emits_kind = "creeper-imminent"
```

`event_extensions` rewrites/derives custom events the agent can subscribe to or use in `on_event` reflexes. They are evaluated by the perception subsystem.

### 8.3 Match precedence

When the foreground window changes, profile detection runs:

1. Walk all loaded profiles in stable order (user-installed first, then bundled).
2. For each profile, walk `matches` array in order.
3. First match wins. A match must satisfy all populated fields (`exe`, `title_regex`, `steam_appid` — fields not specified are wildcards).

The agent can override via `profile_activate(profile_id=...)` regardless of detection.

### 8.4 Bundled profiles at v1

| Profile | Use |
|---|---|
| `notepad` | Windows Notepad (smoke-test productivity app) |
| `vscode` | Visual Studio Code |
| `chrome` | Google Chrome (CDP-enabled when remote debugging port present) |
| `terminal` | Windows Terminal / PowerShell window |
| `file_explorer` | Windows File Explorer |
| `slack` | Slack desktop |
| `discord` | Discord desktop |
| `minecraft.java` | Minecraft Java Edition (single-player) |
| `factorio` | Factorio (no anti-cheat, mod-friendly) |
| `<one FPS>` | TBD — a single-player FPS for the M3 demo (likely a free game) |

Profiles for AC-protected titles ship empty by default (`pixel_only`, no keymap) so the agent can run them in sandbox modes but does nothing competitive-impacting without operator intent.

### 8.5 Profile hot reload

Synapse watches the profile directory via `notify` crate. File changes trigger re-parse and replace in memory. Existing observation streams switch profiles cleanly on the next event tick. Reflexes are not auto-restarted; if a reflex depends on a removed keymap alias, it fails with `REFLEX_PARAMS_INVALID` on next firing.

### 8.6 Versioning

`version` is semver. Profile loader rejects profiles whose major version > Synapse-supported major. Profile minor version is informational; profile authors version their own work.

The expected workflow is community-contributed profiles in a separate repo, installed via a small CLI helper (`synapse-mcp profiles install <repo>/<name>`).

### 8.7 Profile signing (post-v1)

Profiles are TOML data. They cannot execute arbitrary code (no scripts in v1). Even so, post-v1 we plan optional profile signing via a community-key model so the operator can choose to load only signed profiles. Tracked in `16_open_questions.md`.

---

## 9. Migrations

Pre-v1: none. DB wipe on schema change is acceptable. CI tests that wipe-and-rebuild from a sample data set succeeds.

Post-v1: migrations live in `synapse-storage::migrations` with explicit `from -> to` functions. Migration runs are idempotent and resumable. A migration failure halts the daemon with `STORAGE_SCHEMA_MISMATCH`; operator runs `synapse-mcp db migrate` manually.

---

## 10. Backups

`synapse-mcp db backup <out>` uses RocksDB's `CheckpointBuilder` for a hot, consistent snapshot. Output is a directory the operator can tar/zip.

Restore: `synapse-mcp db restore <in>` stops the daemon (via shared lock file), replaces the DB dir, starts the daemon.

Backup size is approximately the live DB size; with default retention it's typically 100–500 MB.

---

## 11. Disk pressure response

If the DB directory's free disk drops below 1 GB:

1. Log `STORAGE_DISK_LOW` and emit a `system-disk-low` event.
2. Aggressively expire `CF_OCR_CACHE`, `CF_TELEMETRY`, `CF_OBSERVATIONS`.
3. If still below 500 MB, halt writes to `CF_EVENTS` (replay log) and surface `STORAGE_WRITE_FAILED` to callers with that code.
4. Below 200 MB: refuse new MCP sessions; existing sessions get a one-line warning event.

The agent can poll `health` to see disk pressure state.

---

## 12. RocksDB tuning (initial)

```rust
let mut opts = Options::default();
opts.create_if_missing(true);
opts.create_missing_column_families(true);
opts.set_max_background_jobs(2);
opts.set_compression_type(DBCompressionType::Lz4);
opts.set_max_open_files(256);
opts.set_keep_log_file_num(8);
opts.set_write_buffer_size(64 * 1024 * 1024);   // 64 MB memtable
opts.set_max_write_buffer_number(3);
opts.set_target_file_size_base(64 * 1024 * 1024);
opts.set_level_zero_file_num_compaction_trigger(4);
```

Per-CF overrides:

- `CF_EVENTS`, `CF_ACTION_LOG`, `CF_REFLEX_AUDIT`: `LZ4` compression, prefix extractor on the time prefix for range scans.
- `CF_MODEL_CACHE`: `None` compression (already compressed binary), 256 MB write buffer.
- `CF_OBSERVATIONS`, `CF_SESSIONS`: `Zstd` (smaller, fewer writes).

Tuning is in `synapse-storage::tuning`. Easy to adjust without touching call sites.

---

## 13. What this doc does NOT cover

- Sled backend specifics → `synapse-storage::sled_impl` (code only at v1; no separate doc)
- Replay tool UI → none at v1; CLI-only output
- Operator config file schema → `14_build_and_packaging.md`
- Per-CF compaction filter implementation → code only
