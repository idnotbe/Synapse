# Dashboard View Author Checklist

Use this checklist for every Command Center view.

- Declare one tier: overview, triage, or drill-down.
- Build from `dashboard/src/primitives` and token utilities from `dashboard/design`.
- State 3-5 questions the view answers in code through `Section questions={...}`.
- Keep raw terminal, JSON, and command output behind collapsed disclosure.
- Show freshness, stale, disconnected, and truncation states explicitly.
- Keep actions next to the attention they resolve.
- Preserve the 8pt rhythm, shared row heights, sticky table headers, and density toggle.
- Keep every control keyboard-reachable with visible focus.
- Use semantic status labels with icon, shape, text, and color.
- Verify zero runtime requests outside `/dashboard/*` and loopback daemon endpoints.
