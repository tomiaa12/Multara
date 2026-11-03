import type { ChatMessage } from "../../domain/types";
import { resolvedInput } from "../../components/chat/timeline-display";

const HTTP_URL = /https?:\/\/[^\s<>"')\]]+/g;

function pushUrl(value: string, seen: Set<string>, out: string[]) {
  const trimmed = value.trim().replace(/[),.;]+$/g, "");
  if (!/^https?:\/\//.test(trimmed) || seen.has(trimmed)) return;
  seen.add(trimmed);
  out.push(trimmed);
}

function walk(value: unknown, seen: Set<string>, out: string[]) {
  if (typeof value === "string") {
    for (const match of value.match(HTTP_URL) ?? []) pushUrl(match, seen, out);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) walk(item, seen, out);
    return;
  }
  if (!value || typeof value !== "object") return;
  for (const item of Object.values(value)) walk(item, seen, out);
}

/** 从已加载时间线的工具输入和结果里收集 http(s) URL，按出现顺序去重。 */
export function inboxBrowserLinks(messages: readonly ChatMessage[] | undefined): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const message of messages ?? []) {
    for (const item of message.timeline ?? []) {
      walk(resolvedInput(item), seen, out);
      walk(item.output, seen, out);
    }
  }
  return out;
}

/** 地址栏：已有协议的原样使用，否则补上 https。 */
export function normalizeBrowserUrl(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}
