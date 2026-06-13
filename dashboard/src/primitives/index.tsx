import { useMemo, type ReactNode } from "react";
import { getCoreRowModel, useReactTable, flexRender, type ColumnDef } from "@tanstack/react-table";
import { Activity, ChevronDown, FileText, TerminalSquare } from "lucide-react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import * as Collapsible from "@/components/ui/collapsible";
import { Button } from "@/components/ui/button";
import { FlagBadge, StatusBadge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { AgentSummary, FleetStatus, ToolCallSummary } from "@/lib/dashboard-state";
import { boundedText, cn, nsToTime, rawText, timeAgo } from "@/lib/utils";

export function AppShell({ sidebar, children }: { sidebar: ReactNode; children: ReactNode }) {
  return (
    <div className="min-h-screen bg-canvas text-primary">
      <div className="mx-auto grid min-h-screen max-w-[var(--layout-content-max)] grid-cols-1 lg:grid-cols-[var(--layout-sidebar)_minmax(0,1fr)]">
        <aside className="border-b border-border bg-surface-1 p-4 lg:border-b-0 lg:border-r">
          {sidebar}
        </aside>
        <main className="min-w-0 p-4 lg:p-6">{children}</main>
      </div>
    </div>
  );
}

export function PageHeader({
  title,
  subtitle,
  actions
}: {
  title: string;
  subtitle: ReactNode;
  actions: ReactNode;
}) {
  return (
    <header className="mb-6 flex flex-col gap-4 border-b border-border pb-4 md:flex-row md:items-end md:justify-between">
      <div className="min-w-0">
        <p className="text-label font-medium uppercase text-muted">Synapse Command Center</p>
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-primary">{title}</h1>
        <div className="mt-2 text-sm text-secondary">{subtitle}</div>
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-2">{actions}</div>
    </header>
  );
}

export function Section({
  title,
  tier,
  questions,
  actions,
  className,
  children
}: {
  title: string;
  tier: "overview" | "triage" | "drill-down";
  questions: string[];
  actions?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section
      className={cn("min-w-0 border-t border-border py-4", className)}
      data-tier={tier}
      data-questions={questions.join(" | ")}
      aria-labelledby={slug(title)}
    >
      <header className="mb-3 flex items-center justify-between gap-3">
        <div>
          <p className="text-label font-medium uppercase text-muted">{tier}</p>
          <h2 id={slug(title)} className="text-lg font-semibold tracking-normal text-primary">
            {title}
          </h2>
          <ul className="sr-only">
            {questions.map((question) => (
              <li key={question}>{question}</li>
            ))}
          </ul>
        </div>
        {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
      </header>
      {children}
    </section>
  );
}

export function StatCard({
  label,
  value,
  delta,
  status = "idle"
}: {
  label: string;
  value: ReactNode;
  delta?: ReactNode;
  status?: FleetStatus;
}) {
  return (
    <article className="min-h-28 rounded-lg border border-border bg-surface-1 p-[var(--density-card-padding)]">
      <div className="flex items-start justify-between gap-3">
        <p className="text-label font-medium uppercase text-muted">{label}</p>
        <StatusBadge status={status} />
      </div>
      <div className="mt-3 font-mono text-metric font-semibold leading-none text-primary">{value}</div>
      {delta ? <div className="mt-3 text-sm text-secondary">{delta}</div> : null}
    </article>
  );
}

export function MetricRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex min-h-[var(--density-row-height)] items-center justify-between gap-4 border-b border-border-subtle py-2 last:border-b-0">
      <span className="text-sm text-muted">{label}</span>
      <span className="min-w-0 truncate text-right font-mono text-sm text-primary">{value}</span>
    </div>
  );
}

export function EmptyState({ title }: { title: string }) {
  return (
    <div className="flex min-h-24 items-center justify-center rounded-lg border border-border bg-surface-1 text-sm text-muted">
      {title}
    </div>
  );
}

export function DataTable<T>({
  data,
  columns,
  getRowId
}: {
  data: T[];
  columns: ColumnDef<T>[];
  getRowId?: (row: T, index: number) => string;
}) {
  const stableColumns = useMemo(() => columns, [columns]);
  const table = useReactTable({
    data,
    columns: stableColumns,
    getCoreRowModel: getCoreRowModel(),
    getRowId
  });
  return (
    <div className="overflow-auto rounded-lg border border-border">
      <table className="w-full min-w-full border-collapse text-sm">
        <thead className="sticky top-0 bg-surface-2">
          {table.getHeaderGroups().map((headerGroup) => (
            <tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => (
                <th key={header.id} className="border-b border-border px-3 py-2 text-left text-label font-medium uppercase text-muted">
                  {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr key={row.id} className="min-h-[var(--density-row-height)] border-b border-border-subtle last:border-b-0 hover:bg-surface-2">
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id} className="max-w-80 px-3 py-2 align-top text-secondary">
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function RawValue({ value, label = "Raw" }: { value: unknown; label?: string }) {
  const text = boundedText(value, 12000);
  return (
    <Collapsible.Root>
      <Collapsible.Trigger asChild>
        <Button variant="ghost" size="sm">
          <ChevronDown aria-hidden="true" className="h-4 w-4" />
          {label}
        </Button>
      </Collapsible.Trigger>
      <Collapsible.Content>
        <pre className="mt-2 max-h-96 overflow-auto rounded-md border border-border bg-surface-2 p-3 font-mono text-xs leading-relaxed text-secondary">
          {text.text}
        </pre>
        <div className="mt-2 flex flex-wrap gap-2">
          {text.flags.map((flag) => (
            <FlagBadge key={flag} tone={flag === "hygiene" ? "danger" : "warn"}>
              {flag}
            </FlagBadge>
          ))}
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
  );
}

export function ToolCallCard({ call }: { call: ToolCallSummary }) {
  const status: FleetStatus = call.lifecycle === "error" ? "stuck" : call.lifecycle === "success" ? "done" : "working";
  return (
    <article className="rounded-lg border border-border bg-surface-1 p-[var(--density-card-padding)]">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Activity aria-hidden="true" className="h-4 w-4 text-info" />
            <h3 className="truncate text-md font-medium tracking-normal text-primary">{call.tool}</h3>
          </div>
          <p className="mt-1 truncate text-sm text-secondary">{call.summary}</p>
        </div>
        <StatusBadge status={status} />
      </div>
      <div className="mt-3 grid gap-2 text-xs text-muted sm:grid-cols-3">
        <span>{call.actor || "actor unknown"}</span>
        <span>{call.target || "no target"}</span>
        <span>{call.time ? nsToTime(call.time) : ""}</span>
      </div>
      <div className="mt-2">
        <RawValue value={call.raw} label="Details" />
      </div>
    </article>
  );
}

export function FleetRow({
  agent,
  selected,
  onSelect
}: {
  agent: AgentSummary;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "grid w-full grid-cols-[minmax(0,1fr)_auto] gap-3 border-b border-border-subtle px-3 py-3 text-left transition-colors last:border-b-0 hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring",
        selected && "bg-surface-2"
      )}
    >
      <span className="min-w-0">
        <span className="block truncate text-sm font-medium text-primary">{agent.id}</span>
        <span className="mt-1 block truncate text-sm text-secondary">{agent.summary}</span>
        <span className="mt-2 flex gap-3 text-xs text-muted">
          <span>{agent.kind}</span>
          <span>{agent.lastSeenMs === undefined ? agent.lifecycle : timeAgo(agent.lastSeenMs)}</span>
          <span>{agent.diffStats.actions} actions</span>
        </span>
      </span>
      <StatusBadge status={agent.status} />
    </button>
  );
}

export function TranscriptTurn({ row }: { row: Record<string, unknown> }) {
  const record = (row.record && typeof row.record === "object" ? row.record : {}) as Record<string, unknown>;
  const text = rawText(record.content_summary || record.source_error || record.parse_error || "");
  return (
    <article className="rounded-lg border border-border bg-surface-1 p-[var(--density-card-padding)]">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <FileText aria-hidden="true" className="h-4 w-4 text-info" />
          <h3 className="truncate text-md font-medium tracking-normal text-primary">
            {rawText(row.spawn_id) || "transcript"}
          </h3>
        </div>
        <FlagBadge>{rawText(record.role || record.event_kind || "event")}</FlagBadge>
      </div>
      <div className="markdown-body text-sm text-secondary">
        <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSanitize]}>
          {text || "No assistant text recorded."}
        </ReactMarkdown>
      </div>
      <div className="mt-2">
        <RawValue value={row} label="Transcript row" />
      </div>
    </article>
  );
}

export function AgentPeek({ agent }: { agent?: AgentSummary }) {
  if (!agent) {
    return <EmptyState title="No agent selected" />;
  }
  return (
    <Tabs defaultValue="timeline">
      <TabsList aria-label="Agent detail surfaces">
        <TabsTrigger value="timeline">Timeline</TabsTrigger>
        <TabsTrigger value="terminal">Terminal</TabsTrigger>
        <TabsTrigger value="raw">Raw</TabsTrigger>
      </TabsList>
      <TabsContent value="timeline">
        <div className="rounded-lg border border-border bg-surface-1 p-[var(--density-card-padding)]">
          <MetricRow label="Session" value={agent.id} />
          <MetricRow label="Kind" value={agent.kind} />
          <MetricRow label="Lifecycle" value={agent.lifecycle} />
          <MetricRow label="Actions" value={agent.diffStats.actions} />
          <MetricRow label="Transcripts" value={agent.diffStats.transcripts} />
          <MetricRow label="Reason" value={agent.reason || "none"} />
        </div>
      </TabsContent>
      <TabsContent value="terminal">
        <div className="rounded-lg border border-border bg-surface-1 p-[var(--density-card-padding)]">
          <div className="flex items-center gap-2 text-sm text-secondary">
            <TerminalSquare aria-hidden="true" className="h-4 w-4 text-muted" />
            No terminal stream attached.
          </div>
        </div>
      </TabsContent>
      <TabsContent value="raw">
        <RawValue value={agent.raw} label="Agent row" />
      </TabsContent>
    </Tabs>
  );
}

function slug(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}
