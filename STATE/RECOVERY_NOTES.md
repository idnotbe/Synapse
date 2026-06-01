# RECOVERY NOTES - Synapse

## Current Resume Point - 2026-06-01T07:16:00-05:00
- #613 is closed and recorded. Implementation commit `e95a656`; evidence/state commit `e792ea0`; RESOLVED evidence https://github.com/ChrisRoyse/Synapse/issues/613#issuecomment-4592454588; closure readback `CLOSED` at `2026-06-01T12:12:21Z`.
- Active issue is #614 `scenario(stress): reality baseline→delta→audit full loop across all sensors`.
  - START comment: https://github.com/ChrisRoyse/Synapse/issues/614#issuecomment-4592477025
  - Live queue after #613 closure: #594 plus #595-#604 and #614-#634.
  - Post-compaction wired MCP readback succeeded with `health`, `storage_inspect`, `reflex_list include_expired=true`, `reflex_history limit=5`, and `observe depth=0`.
- Current next action: inspect the reality baseline/delta/audit implementation and existing tests before launching the isolated repo-built #614 daemon.

## #614 Scope
- Goal: prove the delta-first reality model end to end across every sensor feed.
- Required runtime evidence:
  - real repo-built `synapse-mcp` daemon process/bind/auth/health/session/strict Inspector `tools/list`;
  - real MCP `tools/call` triggers for `reality_baseline`, `observe_delta`, `reality_audit`, and event-producing tools;
  - separate SoT readbacks for `CF_KV/reality/baseline/*`, `reality/head/*`, `reality/delta/*`, `reality/audit/*`, `reality_delta` SSE frames, and physical foreground/focus/UIA/HUD/entity/audio/clipboard/filesystem/diagnostics changes where available;
  - happy path plus missing baseline, stale epoch, profile-change mid-walk, future/overflow `since_seq`, empty/no-change, boundary, and structurally invalid params.

## #613 Manual FSV Evidence
- Final run directory: `.runs\613\subscribe-firehose-fsv-20260601T062230-patched`
- Daemon: PID `32356`, bind `127.0.0.1:7839`, repo release binary, isolated DB/watch/log dirs.
- Precondition: process/socket/auth readback passed; unauth `/health=401`; auth `/health ok=true`; official MCP Inspector strict `tools/list` returned 80 tools with #613 tools present.
- One-per-event: subscription `019e82ec-ebf5-7943-884e-03590d0a05f2` delivered exactly 3 frames for `/focused,/clipboard,/fs`; stream/event seqs `1,2,3`; no drops; file and clipboard physical SoTs matched marker `issue613-patched-oneper-20260601T062403456`.
- 8-deep filter: subscription `019e82ee-5d56-72f2-92c0-00e3c4a73063` accepted regex/in_set/exists filter at max depth and delivered only `/clipboard` and `/fs` from four published reality deltas.
- Firehose/backpressure: subscription `019e82ef-c53f-7e13-ae2c-cfea7dbd3ae8`; 5000 known events posted; stats read `ring_len=4096`, `oldest_event_seq=904`, `latest_event_seq=4999`, `dropped_total=904`, `events_dropped_for_subscriber=904`, `lossy_pending=true`; replay had 1 lossy preface and 4096 event frames.
- Edges: depth 9, invalid regex, invalid data path, and bad buffer size rejected through strict Inspector; empty filter All delivered event seq `613000`; subscribe/immediate cancel produced `cancelled=true`, stats 404 after cancel and after matching publish.
- Cleanup: subscriptions cancelled, `sse_subscribers=0`, `release_all` zero, daemon stopped, port `7839` closed.

## Final Supporting Checks
- `cargo fmt --check`
- `git diff --check` (line-ending warnings only)
- `cargo check -p synapse-core -j 2`
- `cargo check -p synapse-reflex -j 2`
- `cargo check -p synapse-mcp -j 2`
- `cargo test -p synapse-core event_filter_validation_edges_have_readback --test event_filter_types -- --nocapture`
- `cargo test -p synapse-mcp last_event_id_zero_reuses_empty_existing_subscription --bin synapse-mcp -- --nocapture`
- `cargo test -p synapse-mcp ring_overflow_reports_drop_metric_and_lossy_frame --bin synapse-mcp -- --nocapture`
- `cargo test -p synapse-mcp --test m3_subscribe_tool -- --nocapture`
- `cargo test -p synapse-mcp --bin synapse-mcp schema_sanitize -- --nocapture`
- `cargo test -p synapse-reflex --test bus_behavior -- --nocapture`
- `cargo build --release -p synapse-mcp -j 2`
- Release binary: `target\release\synapse-mcp.exe`, length `46359552`, SHA256 `426E96F4CA1C07D92433284FEBD39A161722C256133265AD6472B4E1D51DB67C`, timestamp `2026-06-01T12:09:18.7698237Z`.

## Standing Rules
- Re-read `C:\code\Synapse\docs\AICodingAgentSuperPrompt.md`, `C:\Users\hotra\Downloads\AICodingAgentSuperPrompt.md`, `AGENTS.md`, and `STATE/*` after compaction.
- GitHub Issues are the only coordination surface.
- No GitHub Actions/CI dispatch, waits, or CI-gated claims.
- Commits pushed by this agent must include `[skip ci]`.
- Automated checks/benches are supporting regression evidence only; they are not FSV.
- Missing local prerequisites are acquisition/setup work, not blockers, unless only a specific operator-only hard-to-reverse external action remains.
