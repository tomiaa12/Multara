import { t } from "../../../i18n";

export type Tone = "success" | "error" | "primary" | "warning" | "accent" | "purple" | undefined;

/** 毫秒 -> 「1h 2m」「3m 04s」「12s」「450ms」 */
export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  const total = Math.round(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  if (hours) return `${hours}h ${minutes}m`;
  if (minutes) return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
  return `${seconds}s`;
}

export function timeMs(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const value = new Date(iso).getTime();
  return Number.isFinite(value) ? value : null;
}

/** start..end（end 缺省取 now）的毫秒数；start 缺省返回 null */
export function elapsedMs(start: string | null | undefined, end: string | null | undefined, now = Date.now()): number | null {
  const from = timeMs(start);
  if (from == null) return null;
  return Math.max(0, (timeMs(end) ?? now) - from);
}

export function runStatusTone(status: string): Tone {
  switch (status) {
    case "succeeded": return "success";
    case "failed": return "error";
    case "running": return "primary";
    case "waiting":
    case "queued": return "warning";
    default: return undefined;
  }
}

export function nodeStatusTone(status: string): Tone {
  switch (status) {
    case "succeeded": return "success";
    case "failed": return "error";
    case "running":
    case "waiting_children":
    case "ready": return "primary";
    case "waiting_human": return "warning";
    default: return undefined;
  }
}

export function runStatusLabel(status: string): string {
  const key = `workflow.run.status.${status}`;
  const text = t(key);
  return text === key ? status : text;
}

export function nodeStatusLabel(status: string): string {
  const key = `workflow.nodeStatus.${status}`;
  const text = t(key);
  return text === key ? status : text;
}

export function triggerSourceLabel(source: string): string {
  const key = `workflow.run.source.${source}`;
  const text = t(key);
  return text === key ? source : text;
}

/** 值摘要：单行、截断 */
export function summarizeValue(value: unknown, max = 48): string {
  if (value === null || value === undefined) return "null";
  let text: string;
  if (typeof value === "string") text = value;
  else {
    try {
      text = JSON.stringify(value);
    } catch {
      text = String(value);
    }
  }
  text = text.replace(/\s+/g, " ").trim();
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

/** 入参摘要：`key=value, key=value` */
export function summarizeInputs(inputs: unknown, max = 80): string {
  if (!inputs || typeof inputs !== "object") return summarizeValue(inputs, max);
  const entries = Object.entries(inputs as Record<string, unknown>);
  if (!entries.length) return "—";
  const text = entries.map(([key, value]) => `${key}=${summarizeValue(value, 28)}`).join(", ");
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}
