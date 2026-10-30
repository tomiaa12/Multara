import type { ChatMessage, TimelineItem } from "../../domain/types";
import { t } from "../../i18n";
import {
  exploreDetail,
  inferToolName,
  itemKind,
  toolCommand,
  toolPath,
} from "../chat/timeline-display";

/** 一次工具调用：把 tool_use / tool_result 收成一行。 */
export interface RunLogCallStep {
  kind: "call";
  seq: number;
  tool: string;
  call?: TimelineItem;
  result?: TimelineItem;
  startedAt?: string;
  endedAt?: string;
  durationMs?: number;
}

export interface RunLogMessageStep {
  kind: "text" | "thinking" | "error";
  seq: number;
  item: TimelineItem;
  startedAt?: string;
}

export type RunLogStep = RunLogCallStep | RunLogMessageStep;

export type LaneSegmentKind = "tool" | "think" | "report" | "error";

export interface LaneSegment {
  startMs: number;
  durationMs: number;
  kind: LaneSegmentKind;
}

export interface TraceLanes {
  model: LaneSegment[];
  tool: LaneSegment[];
  modelMs: number;
  toolMs: number;
  totalMs: number;
}

const STATUS_KEYS: Record<string, string> = {
  queued: "agent.run.queued",
  running: "agent.run.running",
  completed: "agent.run.completed",
  failed: "agent.run.failed",
  stopped: "agent.run.stopped",
};

export function runStatusLabel(status: string): string {
  const key = STATUS_KEYS[status];
  return key ? t(key) : status;
}

/** 运行状态对应的徽标色调（ds-badge 的 data-tone）；未知状态返回 undefined 即中性色 */
export function runStatusTone(status: string): "success" | "error" | "primary" | "warning" | undefined {
  if (status === "completed") return "success";
  if (status === "failed") return "error";
  if (status === "running") return "primary";
  if (status === "queued") return "warning";
  return undefined;
}

function timeMs(iso: string | undefined): number | undefined {
  if (!iso) return undefined;
  const ms = new Date(iso).getTime();
  return Number.isFinite(ms) ? ms : undefined;
}

function durationBetween(start?: string, end?: string): number | undefined {
  const a = timeMs(start);
  const b = timeMs(end);
  if (a === undefined || b === undefined) return undefined;
  return Math.max(0, b - a);
}

/** 从会话消息里抽出某次运行的时间线（按 seq / 时间排序）。 */
export function timelineFromMessages(messages: ChatMessage[], runId: string): TimelineItem[] {
  const items = messages
    .filter((message) => message.runId === runId && message.role === "assistant")
    .flatMap((message) => message.timeline);
  return [...items].sort((a, b) => {
    if (a.seq !== b.seq) return a.seq - b.seq;
    return a.createdAt.localeCompare(b.createdAt);
  });
}

/** 取该次运行消息上已有的用时；没有则返回 null（由时间戳兜底）。 */
export function elapsedFromMessages(messages: ChatMessage[], runId: string): number | null {
  const matched = messages.filter((message) => message.runId === runId && message.role === "assistant");
  for (let index = matched.length - 1; index >= 0; index -= 1) {
    const value = matched[index]?.elapsedMs;
    if (typeof value === "number" && Number.isFinite(value) && value >= 0) return value;
  }
  return null;
}

export function failureFromMessages(messages: ChatMessage[], runId: string): string | null {
  const matched = messages.filter((message) => message.runId === runId && message.role === "assistant");
  for (let index = matched.length - 1; index >= 0; index -= 1) {
    const reason = matched[index]?.failureReason?.trim();
    if (reason) return reason;
  }
  return null;
}

/** 折叠 tool_use / tool_result；优先用 callId 配对。 */
export function buildRunLogSteps(items: TimelineItem[]): RunLogStep[] {
  const steps: RunLogStep[] = [];
  const openByCallId = new Map<string, RunLogCallStep>();
  const openByTool = new Map<string, RunLogCallStep[]>();

  for (const item of items) {
    if (item.eventType === "tool_use") {
      const tool = item.tool?.trim() || inferToolName(item) || "Tool";
      const step: RunLogCallStep = {
        kind: "call",
        seq: item.seq,
        tool,
        call: item,
        startedAt: item.createdAt,
      };
      steps.push(step);
      if (item.callId) openByCallId.set(item.callId, step);
      else {
        const queue = openByTool.get(tool) ?? [];
        queue.push(step);
        openByTool.set(tool, queue);
      }
      continue;
    }

    if (item.eventType === "tool_result") {
      const tool = item.tool?.trim() || inferToolName(item) || "Tool";
      let pending = item.callId ? openByCallId.get(item.callId) : undefined;
      if (item.callId && pending) openByCallId.delete(item.callId);
      if (!pending) pending = openByTool.get(tool)?.shift();
      if (pending) {
        pending.result = item;
        pending.endedAt = item.createdAt;
        pending.durationMs = durationBetween(pending.startedAt, item.createdAt);
        if (!pending.tool || pending.tool === "Tool") pending.tool = tool;
        continue;
      }
      steps.push({
        kind: "call",
        seq: item.seq,
        tool,
        result: item,
        startedAt: item.createdAt,
        endedAt: item.createdAt,
      });
      continue;
    }

    if (item.eventType === "thinking" || item.eventType === "error" || item.eventType === "text") {
      steps.push({
        kind: item.eventType,
        seq: item.seq,
        item,
        startedAt: item.createdAt,
      });
    }
  }

  return steps;
}

interface Interval {
  start: number;
  end: number;
}

function mergeIntervals(intervals: Interval[]): Interval[] {
  const sorted = [...intervals].sort((a, b) => a.start - b.start);
  const out: Interval[] = [];
  for (const interval of sorted) {
    const prev = out[out.length - 1];
    if (prev && interval.start <= prev.end) {
      prev.end = Math.max(prev.end, interval.end);
      continue;
    }
    out.push({ ...interval });
  }
  return out;
}

function kindForGap(steps: RunLogStep[], from: number, to: number): LaneSegmentKind {
  let kind: LaneSegmentKind = "think";
  for (const step of steps) {
    if (step.kind === "call") continue;
    const at = timeMs(step.startedAt);
    if (at === undefined || at < from || at > to) continue;
    if (step.kind === "error") return "error";
    if (step.kind === "text") kind = "report";
  }
  return kind;
}

/**
 * 模型 / 工具双色块时间轴。
 * 工具段来自成对调用的墙钟区间；模型段是工具空隙（含头尾）。
 */
export function buildLanes(
  steps: RunLogStep[],
  runStart?: string,
  runEnd?: string,
): TraceLanes | null {
  const stamps = steps
    .flatMap((step) => [
      timeMs(step.startedAt),
      timeMs(step.kind === "call" ? step.endedAt : undefined),
    ])
    .filter((ms): ms is number => ms !== undefined);
  if (stamps.length === 0) return null;

  const startMs = Math.min(timeMs(runStart) ?? Infinity, ...stamps);
  const endMs = Math.max(timeMs(runEnd) ?? -Infinity, ...stamps);
  const totalMs = endMs - startMs;
  if (!Number.isFinite(totalMs) || totalMs <= 0) return null;

  const calls: Interval[] = [];
  for (const step of steps) {
    if (step.kind !== "call") continue;
    const start = timeMs(step.startedAt);
    const end = timeMs(step.endedAt);
    if (start === undefined || end === undefined || end <= start) continue;
    calls.push({
      start: Math.max(startMs, start),
      end: Math.min(endMs, end),
    });
  }

  const merged = mergeIntervals(calls.filter((item) => item.end > item.start));
  const tool: LaneSegment[] = merged.map((item) => ({
    startMs: item.start - startMs,
    durationMs: item.end - item.start,
    kind: "tool" as const,
  }));

  const model: LaneSegment[] = [];
  let cursor = startMs;
  const pushModel = (from: number, to: number) => {
    if (to <= from) return;
    model.push({
      startMs: from - startMs,
      durationMs: to - from,
      kind: kindForGap(steps, from, to),
    });
  };
  for (const interval of merged) {
    pushModel(cursor, interval.start);
    cursor = Math.max(cursor, interval.end);
  }
  pushModel(cursor, endMs);

  return {
    model,
    tool,
    modelMs: model.reduce((sum, item) => sum + item.durationMs, 0),
    toolMs: tool.reduce((sum, item) => sum + item.durationMs, 0),
    totalMs,
  };
}

export const TIMELINE_MIN_STEPS = 2;
export const TIMELINE_MIN_MS = 1_000;

export function shouldShowTimeline(steps: RunLogStep[], lanes: TraceLanes | null): boolean {
  if (!lanes) return false;
  return steps.length >= TIMELINE_MIN_STEPS && lanes.totalMs >= TIMELINE_MIN_MS;
}

export function timelineTicks(totalMs: number, maxTicks = 6): number[] {
  if (!Number.isFinite(totalMs) || totalMs <= 0) return [];
  const candidates = [
    1_000, 2_000, 5_000, 10_000, 15_000, 30_000,
    60_000, 2 * 60_000, 5 * 60_000, 10 * 60_000, 15 * 60_000, 30 * 60_000,
    60 * 60_000, 2 * 60 * 60_000,
  ];
  const step = candidates.find((ms) => totalMs / ms <= maxTicks) ?? candidates[candidates.length - 1]!;
  const ticks: number[] = [];
  for (let at = 0; at <= totalMs + 0.5; at += step) ticks.push(at);
  if (ticks[ticks.length - 1]! < totalMs) ticks.push(totalMs);
  return ticks;
}

export function formatElapsedMs(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (minutes < 60) return `${minutes}m ${secs}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function formatLaneTotal(ms: number): string {
  return formatElapsedMs(ms);
}

export function formatStepDuration(ms: number): string {
  if (ms < 1000) return `${(ms / 1000).toFixed(1)}s`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
}

export function formatOffset(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) return `+${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  const hours = Math.floor(minutes / 60);
  return `+${hours}:${String(minutes % 60).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export function formatTick(ms: number): string {
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) return seconds === 0 ? `${minutes}m` : `${minutes}m ${seconds}s`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

export function formatRunTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString("zh-CN", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function laneSegmentStyle(segment: LaneSegment, totalMs: number): Record<string, string> {
  if (totalMs <= 0) return { left: "0%", width: "0%" };
  const left = (segment.startMs / totalMs) * 100;
  const width = Math.max(0.35, (segment.durationMs / totalMs) * 100);
  return {
    left: `${left}%`,
    width: `${Math.min(100 - left, width)}%`,
  };
}

export function stepTitle(step: RunLogStep): string {
  if (step.kind === "call") return step.tool || inferToolName(step.call ?? step.result!) || "Tool";
  if (step.kind === "thinking") return t("runLog.step.thinking");
  if (step.kind === "error") return t("runLog.step.error");
  return t("runLog.step.reply");
}

export function stepSummary(step: RunLogStep): string {
  if (step.kind !== "call") {
    const text = (step.item.content ?? "").trim().replace(/\s+/g, " ");
    return text.length > 120 ? `${text.slice(0, 120)}…` : text;
  }
  const item = step.call ?? step.result;
  if (!item) return "";
  const command = toolCommand(item);
  if (command) {
    const one = command.trim().replace(/\s+/g, " ");
    return one.length > 100 ? `${one.slice(0, 100)}…` : one;
  }
  const path = toolPath(item);
  if (path) return path;
  return exploreDetail(item);
}

const inputJsonCache = new WeakMap<object, string>();

/** 工具入参在 payload 重建时通常是同一个对象，按对象缓存序列化结果，搜索按键时不必重复 stringify。 */
function inputJson(input: unknown): string {
  if (!input) return "";
  if (typeof input !== "object") return JSON.stringify(input);
  const cached = inputJsonCache.get(input);
  if (cached !== undefined) return cached;
  const text = JSON.stringify(input);
  inputJsonCache.set(input, text);
  return text;
}

export function stepHaystack(step: RunLogStep): string {
  if (step.kind !== "call") return (step.item.content ?? "").toLowerCase();
  const input = inputJson(step.call?.input);
  const output = (step.result?.output ?? step.result?.content ?? "").slice(0, 4000);
  return `${step.tool} ${input} ${output}`.toLowerCase();
}

export function stepDetailText(step: RunLogStep): string {
  if (step.kind !== "call") return step.item.content ?? "";
  const parts: string[] = [];
  if (step.call?.input) parts.push(`${t("runLog.detail.input")}\n${JSON.stringify(step.call.input, null, 2)}`);
  else if (step.call?.content) parts.push(`${t("runLog.detail.input")}\n${step.call.content}`);
  const output = step.result?.output ?? step.result?.content;
  if (output) parts.push(`${t("runLog.detail.result")}\n${output}`);
  return parts.join("\n\n");
}

export function stepKindClass(step: RunLogStep): string {
  if (step.kind === "call") return itemKind(step.call ?? step.result!) || "other";
  if (step.kind === "thinking") return "thinking";
  if (step.kind === "error") return "error";
  return "text";
}

export function resolveRunDurationMs(options: {
  elapsedMs?: number | null;
  createdAt: string;
  endedAt?: string | null;
  steps: RunLogStep[];
  live?: boolean;
  nowMs?: number;
}): number {
  if (typeof options.elapsedMs === "number" && Number.isFinite(options.elapsedMs) && options.elapsedMs >= 0) {
    return options.elapsedMs;
  }
  const start = timeMs(options.createdAt);
  if (start === undefined) return 0;
  if (options.live) {
    return Math.max(0, (options.nowMs ?? Date.now()) - start);
  }
  const end =
    timeMs(options.endedAt ?? undefined)
    ?? Math.max(
      ...options.steps.flatMap((step) => [
        timeMs(step.startedAt) ?? 0,
        step.kind === "call" ? timeMs(step.endedAt) ?? 0 : 0,
      ]),
      start,
    );
  return Math.max(0, end - start);
}

/** 连续 3 次及以上、且不是终端命令的同类工具调用，折成一组。 */
export const MIN_GROUP_SIZE = 3;

export interface RunLogGroup {
  kind: "group";
  seq: number;
  tool: string;
  steps: RunLogCallStep[];
  startedAt?: string;
  endedAt?: string;
  durationMs?: number;
}

export type RunLogRow = RunLogStep | RunLogGroup;

function isShellCall(step: RunLogCallStep): boolean {
  return Boolean(toolCommand(step.call ?? step.result));
}

export function isGroupRow(row: RunLogRow): row is RunLogGroup {
  return row.kind === "group";
}

export function groupRunLogSteps(steps: RunLogStep[]): RunLogRow[] {
  const rows: RunLogRow[] = [];
  let run: RunLogCallStep[] = [];

  const flush = () => {
    if (run.length === 0) return;
    if (run.length < MIN_GROUP_SIZE) {
      rows.push(...run);
    } else {
      const first = run[0]!;
      const last = run[run.length - 1]!;
      rows.push({
        kind: "group",
        seq: first.seq,
        tool: first.tool,
        steps: run,
        startedAt: first.startedAt,
        endedAt: last.endedAt,
        durationMs: durationBetween(first.startedAt, last.endedAt),
      });
    }
    run = [];
  };

  for (const step of steps) {
    if (step.kind === "call" && !isShellCall(step) && (run.length === 0 || run[0]!.tool === step.tool)) {
      run.push(step);
      continue;
    }
    flush();
    if (step.kind === "call") run.push(step);
    else rows.push(step);
  }
  flush();
  return rows;
}

export interface RunOutcome {
  paths: string[];
  commandCount: number;
}

/** 这次运行改过的文件和跑过的命令，没有可汇总内容时不展示产出条。 */
export function buildRunOutcome(steps: RunLogStep[]): RunOutcome | null {
  const paths: string[] = [];
  const seen = new Set<string>();
  let commandCount = 0;
  for (const step of steps) {
    if (step.kind !== "call") continue;
    const item = step.call ?? step.result;
    if (!item) continue;
    if (toolCommand(item)) commandCount += 1;
    const path = toolPath(item);
    if (path && !seen.has(path)) {
      seen.add(path);
      paths.push(path);
    }
  }
  if (paths.length === 0 && commandCount === 0) return null;
  return { paths, commandCount };
}

export type RunLogFilterKey = "text" | "thinking" | "error" | `tool:${string}`;

export function stepFilterKey(step: RunLogStep): RunLogFilterKey {
  return step.kind === "call" ? `tool:${step.tool}` : step.kind;
}
