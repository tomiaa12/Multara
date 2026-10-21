import type { UsageDay } from "../domain/types";

export function emptyUsageDay(date: string): UsageDay {
  return {
    date,
    total: 0,
    failed: 0,
    seconds: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheWriteTokens: 0,
    costUsd: 0,
  };
}

export function totalTokens(item: {
  inputTokens?: number;
  outputTokens?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
}) {
  return (item.inputTokens ?? 0) + (item.outputTokens ?? 0) + (item.cacheReadTokens ?? 0) + (item.cacheWriteTokens ?? 0);
}

export function addUsageDays(left: UsageDay, right: UsageDay): UsageDay {
  return {
    date: left.date,
    total: left.total + right.total,
    failed: left.failed + right.failed,
    seconds: left.seconds + right.seconds,
    inputTokens: left.inputTokens + right.inputTokens,
    outputTokens: left.outputTokens + right.outputTokens,
    cacheReadTokens: left.cacheReadTokens + right.cacheReadTokens,
    cacheWriteTokens: left.cacheWriteTokens + right.cacheWriteTokens,
    costUsd: left.costUsd + right.costUsd,
  };
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0分钟";
  if (seconds < 60) return "<1分钟";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}分钟`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours}小时${rest}分钟` : `${hours}小时`;
}

export function formatUsd(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "$0.00";
  if (value >= 100) return `$${value.toFixed(0)}`;
  if (value < 0.01) return `$${value.toFixed(4)}`;
  return `$${value.toFixed(2)}`;
}

export function formatTokens(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1)}M`;
  if (value >= 1000) return `${(value / 1000).toFixed(value >= 10_000 ? 0 : 1)}k`;
  return String(Math.round(value));
}
