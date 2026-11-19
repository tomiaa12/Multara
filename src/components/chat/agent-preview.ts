import type { TimelineItem } from "../../domain/types";
import { itemKind, resolvedInput, stringField, toolPath } from "./timeline-display";

export type AgentPreviewRequest =
  | { type: "file"; path: string; startLine?: number; endLine?: number }
  | { type: "browser"; url: string };

const BROWSER_TOOL = /navigate|open_url|browser_navigate|goto|openurl/i;

/** 从一条时间线事件推断要主动打开的右侧预览（文件 / 浏览器）。
 *  Write/Edit 不自动开编辑器；仅 Open/Reveal 等明确打开工具，以及浏览器导航。
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

  // Write/Edit 只改磁盘，不抢编辑区；仅明确的「打开/展示」类工具才主动打开文件。
  // 读/搜太吵，也不主动抢焦点。用户可点 FileDiff 卡片自行打开。
  if (item.eventType === "tool_result" && isIntentionalFileOpenTool(item.tool)) {
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

/** Open / Reveal / ShowInEditor 等；按工具名识别，避免被 itemKind 误判成 read。 */
function isIntentionalFileOpenTool(tool: string | null | undefined) {
  const n = (tool ?? "").toLowerCase().replaceAll(/[^a-z]/g, "");
  if (!n) return false;
  // 排除 write/edit/read 本体，避免 WriteFile、ReadFile 误命中
  if (n.includes("write") || n.includes("edit") || n.includes("read") || n.includes("search")) {
    return false;
  }
  return (
    n === "open"
    || n === "openfile"
    || n === "reveal"
    || n === "revealfile"
    || n.includes("showineditor")
    || n.includes("openineditor")
  );
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
