# Delta-First Reality Tool Boundary

Issue: #656

Decision: keep `observe` separate from `reality_baseline`, `observe_delta`, and
`reality_audit`.

`observe` is the M1 perception snapshot tool. It reads the current machine state
and returns an `Observation` with controls for include slots, depth, element
paging, subtree roots, and event filtering. It is the right tool when the caller
needs the current physical view.

The delta-first reality trio is a durable state protocol:

- `reality_baseline` establishes or reuses an epoch and writes the compact
  baseline/head rows under CF_KV.
- `observe_delta` observes physical state, appends ordered delta rows, publishes
  reality-delta events, and returns a cursor.
- `reality_audit` re-reads physical state, compares it to the stored assumption,
  persists drift findings, and reports whether a rebase is required.

They should stay separate for these concrete reasons:

- Different side-effect class: the reality tools require storage/event
  permissions and intentionally write/read CF_KV rows; `observe` should not hide
  those durable mutations behind a perception call.
- Different response contracts: baseline, delta, and audit each return
  branch-specific envelopes with readback rows, cursors, drift status, or rebase
  guidance. A single `observe { mode }` response would mostly be nullable union
  fields while preserving all branch complexity.
- Different lifecycle: baseline creates or reuses an epoch, delta appends after a
  cursor, and audit compares an assumption hash against a fresh physical read.
  Those are ordered protocol steps, not alternate snapshot views.
- Client safety: callers can use `observe` for a present-state read without
  advancing the delta model; callers that mutate the delta model must ask for a
  reality tool explicitly.

The implementation should still share internals where that reduces duplication.
`capture_reality_observation` already reuses the M1 observation assembly path to
produce compact reality state. Future cleanup can factor common include/depth
validation or capture helpers, but the public MCP tool boundary stays split
unless real client usage shows a clearer migration path.
