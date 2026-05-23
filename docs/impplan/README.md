# impplan — Synapse Implementation Plan

Operational map from PRD (`docs/computergames/`) → code. Each phase is a binary deliverable with a hard demo gate. Files in this directory are **prescriptive**; PRD is descriptive. Conflict ⇒ PRD wins, file is patched.

Doctrine: `docs2/compressionprompt.md` §0-13. Keep verbatim: paths, crate names, error codes, thresholds, deps. Cut meta-framing, restatement, motivation prose — PRD already says it.

**Global design invariant (OQ-004 DECIDED 2026-05-22): `Natural` curves + `Natural` keystroke dynamics are the default everywhere, tuned `FAST` (50 ms `Snap` travel, ~190 WPM typing). No `Instant` jumps, no `Burst` typing as defaults. See `07_cross_cutting.md` §12.**

---

## Phase index

| # | File | Phase | PRD demo gate | Effort (solo) |
|---|---|---|---|---|
| 00 | [`00_methodology.md`](00_methodology.md) | Dev discipline (all phases) | n/a | — |
| 01 | [`01_m0_bootstrap.md`](01_m0_bootstrap.md) | M0 — workspace + rmcp stdio + `health` | `15_roadmap_and_milestones.md` §2 | 1w |
| 02 | [`02_m1_perception_mvp.md`](02_m1_perception_mvp.md) | M1 — capture + UIA + `observe()` | §3 | 2-3w |
| 03 | [`03_m2_action_mvp.md`](03_m2_action_mvp.md) | M2 — input emit + `ReleaseAll` | §4 | 2w |
| 04 | [`04_m3_reflex_mcp_surface.md`](04_m3_reflex_mcp_surface.md) | M3 — reflexes + RocksDB + profiles + HTTP/SSE | §5 | 2-3w |
| 05 | [`05_m4_hardware_hid_first_game.md`](05_m4_hardware_hid_first_game.md) | M4 — RP2040 firmware + `minecraft.java` | §6 | 2-3w |
| 06 | [`06_m5_production_polish.md`](06_m5_production_polish.md) | M5 — installer + 5 profiles + overlay + soak | §7 | 3-4w |
| 07 | [`07_cross_cutting.md`](07_cross_cutting.md) | Perf gates, security, observability, release | §10/§11/§12/§14 | — |

Total: ~14w solo to v1.0. Each phase is merge-blocked by the prior phase's demo gate.

---

## How to use

1. Read PRD top-to-bottom once: `docs/computergames/README.md` → `00` → `01` → ... → `17`.
2. Open the impplan file for the current phase.
3. Walk **Work-items** in order. Each is one merge-sized PR.
4. Block merge on **Acceptance gates** before opening the next phase.
5. **Open Questions** (`16_open_questions.md`) hit during the phase → ADR or defer; do not silently decide.

A work-item is "done" iff:

- Code compiles `cargo build --release --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- `cargo test --workspace` green
- The work-item's specific acceptance bullet passes
- Tracing instrumented, error codes from `synapse-core::error_codes`
- No `unwrap()` outside `#[cfg(test)]`, no `unsafe` outside allowed crates

---

## Per-PR contract (every PR, every phase)

```
✓ Compiles release + dev
✓ Clippy zero warnings (workspace + all-targets)
✓ Tests pass (`cargo test --workspace`)
✓ Files ≤ 500 LoC; functions ≤ 30 LoC; cyclomatic ≤ 10
✓ Error variants carry SCREAMING_SNAKE_CASE code()
✓ Public APIs / CF names are `pub const`
✓ Tracing spans on every non-trivial fn
✓ No mocks gating completion (real captures, real RocksDB)
✓ Schema change ⇒ wipe-and-rebuild (pre-v1)
✓ Bench delta ≤ 20% on tracked metrics (10_performance_budget §14)
✓ Docs cross-refs intact (broken link ⇒ CI fail)
```

---

## Cross-references

| Concern | Authority |
|---|---|
| Crate boundaries, threading, channels | `01_architecture.md` |
| Tool schemas, error response shape, transports | `05_mcp_tool_surface.md`, `06_data_schemas.md` §8 |
| Storage CFs, TTLs, GC layers, profile TOML | `07_storage_and_profiles.md` |
| AC policy + tier gating | `08_anti_cheat_policy.md` |
| Latency budgets per stage / per tool | `10_performance_budget.md` §2/§12 |
| Permissions, redaction, kill switches | `11_security_and_safety.md` |
| Tracing, metrics, OTLP, dashboards | `12_observability.md` |
| Test pyramid, fakes, fuzz, soak | `13_testing_strategy.md` |
| Workspace deps + profiles + features | `14_build_and_packaging.md` |
| Risks per phase | `15_roadmap_and_milestones.md` §9 |
| Open decisions | `16_open_questions.md` |

---

## Out of scope for impplan

- ADR contents (lives in `docs/adr/NNN-*.md`, created when an OQ resolves)
- Issue tracker / sprint board
- User-facing guide (`USER_GUIDE.md`, M5)
- Release notes (per-tag, not per-plan)
