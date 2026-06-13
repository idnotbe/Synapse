import { asArray, asRecord, rawText } from "@/lib/utils";

export interface DashboardPanel<T = unknown> {
  status: "ok" | "error" | "unavailable";
  source: string;
  data: T;
  error?: string;
}

export interface DashboardState {
  schema_version: number;
  generated_at_unix_ms: number;
  bind_addr: string;
  token_policy: string;
  daemon: DashboardPanel;
  sessions: DashboardPanel;
  lease: DashboardPanel;
  storage: DashboardPanel;
  command_audit: DashboardPanel;
  approvals: DashboardPanel;
  suggestions: DashboardPanel;
  armed_runs: DashboardPanel;
  agent_transcripts: DashboardPanel;
  hygiene: DashboardPanel;
  local_models: DashboardPanel;
}

export interface AgentSummary {
  id: string;
  kind: string;
  lifecycle: string;
  status: FleetStatus;
  summary: string;
  lastSeenMs?: number;
  lastAction?: string;
  target?: string;
  reason?: string;
  diffStats: {
    events: number;
    transcripts: number;
    actions: number;
  };
  raw: unknown;
}

export type FleetStatus =
  | "working"
  | "idle"
  | "ready_for_review"
  | "needs_input"
  | "awaiting_approval"
  | "stuck"
  | "done";

export interface ToolCallSummary {
  id: string;
  tool: string;
  lifecycle: "pending" | "running" | "success" | "error";
  summary: string;
  actor?: string;
  target?: string;
  time?: string;
  raw: unknown;
}

export async function fetchDashboardState(): Promise<DashboardState> {
  const response = await fetch("/dashboard/state.json", { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`dashboard state failed: ${response.status}`);
  }
  return response.json();
}

export function panelData<T = Record<string, unknown>>(panel?: DashboardPanel): T {
  return (panel?.data ?? {}) as T;
}

export function buildAgents(state?: DashboardState): AgentSummary[] {
  if (!state) return [];
  const sessionData = asRecord(state.sessions.data);
  const sessionRows = asArray<Record<string, unknown>>(sessionData.sessions);
  const unbound = asArray<Record<string, unknown>>(sessionData.unbound_agent_states);
  const transcripts = asArray<Record<string, unknown>>(asRecord(state.agent_transcripts.data).rows);
  const actions = asArray<Record<string, unknown>>(asRecord(state.command_audit.data).rows);

  const transcriptCounts = new Map<string, number>();
  for (const row of transcripts) {
    const spawnId = rawText(row.spawn_id);
    if (spawnId) transcriptCounts.set(spawnId, (transcriptCounts.get(spawnId) ?? 0) + 1);
  }

  const actionCounts = new Map<string, number>();
  for (const row of actions) {
    const target = rawText(row.target_session_id);
    const actor = rawText(row.actor_session_id);
    for (const id of [target, actor]) {
      if (id) actionCounts.set(id, (actionCounts.get(id) ?? 0) + 1);
    }
  }

  const live = sessionRows.map((row) => {
    const agentState = asRecord(row.agent_state);
    const sessionId = rawText(row.session_id);
    const stateName = rawText(agentState.state || row.lifecycle);
    const lastSeenMs = Number(row.last_seen_ms_ago);
    const lastAction = rawText(row.last_action);
    return {
      id: sessionId,
      kind: rawText(row.agent_kind || row.client_name || "agent"),
      lifecycle: rawText(row.lifecycle),
      status: statusFromLiveSession(stateName, lastSeenMs, lastAction),
      summary: lastAction || stateName || "session live",
      lastSeenMs: Number.isFinite(lastSeenMs) ? lastSeenMs : undefined,
      lastAction,
      target: row.active_target ? rawText(row.active_target) : "",
      reason: rawText(agentState.reason_code),
      diffStats: {
        events: 1,
        transcripts: transcriptCounts.get(sessionId) ?? 0,
        actions: actionCounts.get(sessionId) ?? 0
      },
      raw: row
    } satisfies AgentSummary;
  });

  const historical = unbound.map((row) => {
    const id = rawText(row.session_id || row.spawn_id || row.anchor);
    const stateName = rawText(row.state);
    const reason = rawText(row.reason_code);
    return {
      id,
      kind: rawText(row.agent_kind || "agent"),
      lifecycle: stateName || "unbound",
      status: statusFromHistorical(stateName, reason),
      summary: [stateName, reason].filter(Boolean).join(" / ") || "historical state",
      lastSeenMs: undefined,
      lastAction: "",
      target: "",
      reason,
      diffStats: {
        events: 1,
        transcripts: transcriptCounts.get(id) ?? 0,
        actions: actionCounts.get(id) ?? 0
      },
      raw: row
    } satisfies AgentSummary;
  });

  return [...live, ...historical].filter((agent) => agent.id);
}

export function buildToolCalls(state?: DashboardState): ToolCallSummary[] {
  if (!state) return [];
  const rows = asArray<Record<string, unknown>>(asRecord(state.command_audit.data).rows);
  return rows.slice(0, 16).map((row, index) => {
    const error = rawText(row.error_code);
    const outcome = rawText(row.outcome);
    return {
      id: rawText(row.key_hex) || `${index}`,
      tool: rawText(row.tool || row.verb || "tool"),
      lifecycle: error ? "error" : outcome === "ok" ? "success" : "running",
      summary: [rawText(row.verb), outcome, error].filter(Boolean).join(" / ") || "tool call",
      actor: rawText(row.actor_session_id),
      target: rawText(row.target_session_id),
      time: rawText(row.ts_ns),
      raw: row
    };
  });
}

function statusFromLiveSession(stateName: string, lastSeenMs: number, lastAction: string): FleetStatus {
  if (Number.isFinite(lastSeenMs) && lastSeenMs > 300000) return "stuck";
  if (/approval/i.test(lastAction)) return "awaiting_approval";
  if (/wait|inbox|input/i.test(lastAction)) return "needs_input";
  if (/review/i.test(lastAction)) return "ready_for_review";
  if (/idle/i.test(stateName)) return "idle";
  return "working";
}

function statusFromHistorical(stateName: string, reason: string): FleetStatus {
  if (/failed|unhealthy|missing|error|interrupted|timeout/i.test(`${stateName} ${reason}`)) return "stuck";
  if (/dead|done|exited|closed/i.test(stateName)) return "done";
  return "idle";
}
