import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function rawText(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value);
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function stripTerminalSequences(value: unknown) {
  const before = rawText(value);
  let text = before;
  text = text.replace(/\x1b\][\s\S]*?(?:\x07|\x1b\\)/g, "");
  text = text.replace(/\x9d[\s\S]*?(?:\x07|\x9c)/g, "");
  text = text.replace(/\x1b[P^_][\s\S]*?\x1b\\/g, "");
  text = text.replace(/[\x90\x9e\x9f][\s\S]*?\x9c/g, "");
  text = text.replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "");
  text = text.replace(/\x9b[0-?]*[ -/]*[@-~]/g, "");
  text = text.replace(/\x1b[ -/]*[@-~]/g, "");
  text = text.replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x9f]/g, "");
  return { text, stripped: text !== before };
}

export function suspiciousText(value: string): boolean {
  return /<\s*script|javascript\s*:|onerror\s*=|onload\s*=|ignore\s+(all\s+)?(previous|prior)\s+instructions|reveal\s+(your|the)\s+instructions/i.test(value);
}

export function boundedText(value: unknown, limit = 4096) {
  const stripped = stripTerminalSequences(value);
  const flags: Array<"control-stripped" | "hygiene" | "truncated"> = [];
  let text = stripped.text;
  if (stripped.stripped) flags.push("control-stripped");
  if (suspiciousText(text)) flags.push("hygiene");
  if (text.length > limit) {
    text = text.slice(0, limit);
    flags.push("truncated");
  }
  return { text, flags };
}

export function timeAgo(ms?: number | null): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) return "unknown";
  if (ms < 1000) return "now";
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h`;
}

export function unixMsToTime(value?: number | null): string {
  if (!value || !Number.isFinite(value)) return "";
  return new Date(value).toLocaleTimeString();
}

export function nsToTime(value?: number | string | null): string {
  const ns = Number(value || 0);
  if (!Number.isFinite(ns) || ns <= 0) return "";
  return new Date(Math.floor(ns / 1000000)).toLocaleTimeString();
}

export function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

export function asArray<T = unknown>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : [];
}
