import type { TimelineItem } from "../../domain/types";
import { itemKind, resolvedInput, stringField, toolPath } from "./timeline-display";

export type AgentPreviewRequest =
  | { type: "file"; path: string; startLine?: number; endLine?: number }
  | { type: "browser"; url: string };

const BROWSER_TOOL = /navigate|open_url|browser_navigate|goto|openurl/i;

/** 从一条时间线事件推断要主动打开的右侧预览（文件 / 浏览器）。
 *  Shell 工具不再拉开空终端；长跑服务用 `multara terminal run`。
 */
export function previewFromTimelineItem(item: TimelineItem | null | undefined): AgentPreviewRequest | null {
  if (!item) return null;
  const kind = itemKind(item);
  const input = resolvedInput(item);

  const url = stringField(input, ["url"]).trim();
  if (
    url
    && /^https?:\/\//i.test(url)
    && (kind === "browser" || BROWSER_TOOL.test(item.tool ?? ""))
  ) {
    return { type: "browser", url };
  }

  // 写/改完成后打开文件；读操作太吵，不主动抢焦点。
  if (
    item.eventType === "tool_result"
    && (kind === "write" || kind === "edit")
  ) {
    const path = toolPath(item).trim();
    if (!path) return null;
    const startLine = numberish(input, ["start_line", "startLine", "line", "lineNumber"]);
    const endLine = numberish(input, ["end_line", "endLine", "to_line", "toLine"]) ?? startLine;
    return {
      type: "file",
      path,
      ...(startLine != null ? { startLine, endLine } : {}),
    };
  }

  return null;
}

function numberish(input: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = input[key];
    if (typeof value === "number" && Number.isFinite(value) && value > 0) return Math.floor(value);
    if (typeof value === "string" && value.trim() && Number.isFinite(Number(value))) {
      const n = Number(value);
      if (n > 0) return Math.floor(n);
    }
  }
  return undefined;
}
