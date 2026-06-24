# Issue 845 Timeline Acceptance FSV - 2026-06-24

Issue: https://github.com/ChrisRoyse/Synapse/issues/845

This transcript records the manual full-state verification run for the operator
timeline acceptance issue. The run used the installed Synapse HTTP MCP daemon on
`127.0.0.1:7700`, not source-only tests or fixtures.

## Environment

- Repo: `C:\code\synapse`
- Branch: `main`
- Daemon PID: `21364`
- Tool count: `158`
- Tool surface SHA256: `37564d941b413e7031f3e93f3762d545e56d65c2f2e99dfb8f224ecb2687e4e6`
- Chrome bridge status: `ok`
- Bridge build: `synapse-chrome-bridge-2026-06-24-mousedown-click-v3`
- FSV MCP session: `83d5ae63-fb52-4def-b253-898f1d640017`
- Unique marker: `issue845-fsv-20260624-095507`

The FSV session was explicitly ended at completion. `session_end` reported
`failure_count=0` and `marked_terminated=true`.

## Production Timeline Readback

`timeline_stats` over production `CF_TIMELINE` completed without a scan budget
stop:

- `scan_complete=true`
- `total_rows=4612`
- Rows by UTC day:
  - `2026-06-14`: `280`
  - `2026-06-15`: `638`
  - `2026-06-16`: `400`
  - `2026-06-17`: `233`
  - `2026-06-18`: `186`
  - `2026-06-19`: `40`
  - `2026-06-20`: `416`
  - `2026-06-21`: `38`
  - `2026-06-22`: `541`
  - `2026-06-23`: `705`
  - `2026-06-24`: `1135`
- Rows by kind:
  - `browser_nav`: `477`
  - `demo_marker`: `773`
  - `focus_change`: `526`
  - `idle_end`: `205`
  - `idle_start`: `328`
  - `interaction_summary`: `1216`
  - `purge`: `1`
  - `session_end`: `104`
  - `session_start`: `153`
  - `title_change`: `829`

This verifies a real multi-day operator timeline exists in the physical
timeline column family and that aggregation reconciles without invalid rows or
an incomplete scan.

## Search, Get, Stats, And Purge

The run inserted four bounded probe rows into `CF_TIMELINE` through the live
MCP tool `storage_put_probe_rows` using valid timeline keys and `browser_nav`
records:

- `cf_name=CF_TIMELINE`
- `key_prefix=issue845-fsv-20260624-095507`
- `rows_added=4`
- `app=issue845-fsv.exe`
- payload URL: `https://synapse.local/issue845-fsv-20260624-095507`

Readback:

- `timeline_search { text }`: `4` matches
- `timeline_search { apps, kinds, actor }`: `4` matches
- `timeline_get { start_ts_ns, end_ts_ns, kinds, actor }`: `4` rows
- `timeline_stats { start_ts_ns, end_ts_ns }`: `4` rows

Purge controls:

- `timeline_purge { text, dry_run=true }`: `matched_rows=4`, `deleted_rows=0`
- `timeline_purge { text, dry_run=false }`: `matched_rows=4`, `deleted_rows=4`, `compacted=true`
- Purge audit key: `18bc0c2836d4b644ffff0001`
- Post-purge `timeline_search { text, kinds=["browser_nav"] }`: `0` matches

The post-purge search was scoped to `browser_nav` because the purge audit row
correctly preserves the original filter text.

## Pause, Resume, And Exclusions

The FSV called the live MCP controls and verified both tool readback and
`timeline_stats` control-state readback:

- `timeline_pause { duration_ms=60000 }`:
  - `paused=true`
  - `persisted=true`
  - `boundary_row_written=true`
- `timeline_stats` while paused:
  - `recorder.paused=true`
- `timeline_resume {}`:
  - `paused=false`
  - `persisted=true`
  - `boundary_row_written=true`
- `timeline_stats` after resume:
  - `recorder.paused=false`
- `timeline_exclusions { add=["issue845-fsv-excluded.exe"] }`:
  - added entry appeared in `effective_exclusions`
- `timeline_exclusions { remove=["issue845-fsv-excluded.exe"] }`:
  - removed entry no longer appeared in runtime exclusions

## Storage GC

The run selected an empty probe column family and verified actual row eviction
instead of running GC against production history:

- Selected CF: `CF_TELEMETRY`
- `storage_put_probe_rows`: seeded `5` rows
- `storage_gc_once { cf_name="CF_TELEMETRY", soft_cap_rows=1, hard_cap_rows=5 }`:
  - `before_rows=5`
  - `after_rows=1`
  - `total_evicted_rows=4`
- `storage_inspect` after GC:
  - `CF_TELEMETRY` row count was `1`

This proves the GC row-cap path deleted rows from physical storage while
keeping the mutation scoped to diagnostic probe data.

## Demo Mode

Demo recording was exercised in the same live FSV session:

- `demo_record_start`:
  - `demo_id=demo.timeline-fsv.63707ab26fcc5310`
  - `marker_row_written=true`
- `demo_record_stop`:
  - replay path: `C:\Users\hotra\AppData\Local\synapse\replays\demo-recordings\demo.timeline-fsv.63707ab26fcc5310.jsonl`
  - `records_written=5`
  - `event_rows_exported=3`
  - `bytes=4083`
  - `cleared_active_state=true`

## Acceptance Mapping

- Recorder correctness and real timeline capture: PASS, production
  `timeline_stats` over `CF_TIMELINE` returned 4612 rows across 11 UTC days.
- Cadence: PASS, `interaction_summary=1216` in production stats.
- Enrichments: PASS, production stats included `browser_nav=477`,
  `demo_marker=773`, `idle_start=328`, `idle_end=205`, focus/title/session
  rows, and seeded probe payload search over app/title/URL worked.
- Search/get/stats: PASS, seeded rows returned exactly through all three read
  surfaces.
- Pause/exclusion controls: PASS, live controls persisted and stats reflected
  paused/resumed state.
- Purge: PASS, dry-run and destructive purge matched and deleted exactly the
  seeded rows, with an audit row retained.
- GC: PASS, diagnostic probe rows in `CF_TELEMETRY` were evicted from 5 rows to
  1 row.
- Demo mode: PASS, start/stop wrote markers, exported replay JSONL, and cleared
  active state.
