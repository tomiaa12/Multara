// 工具结果带 UI（方案 §4.4）：智能体跑 `multara ... --ui`，输出 JSON 里带 `_ui` 块时，
// 正文下方直接显示成界面块。不改后端、不加列，从时间线现读现算。
import type { TimelineItem } from "../../../domain/types";
import { itemKind, toolCommand } from "../timeline-display";
import { parseSpecs, type GenUiSpec } from "./block";

export interface ToolUi {
  callId: string;
  command: string;
  spec: GenUiSpec;
  raw: string;
}

/** shell 命令以 multara（或 $MULTARA_CLI）开头 */
function isMultaraCommand(command: string) {
  const first = command.trim().split(/\s+/)[0] ?? "";
  const name = first.replace(/^["']|["']$/g, "");
  return /(^|\/)multara(\.exe)?$/.test(name) || name === "$MULTARA_CLI" || name === "${MULTARA_CLI}";
}

export function toolResultUis(timeline: TimelineItem[]): ToolUi[] {
  const uses = new Map<string, TimelineItem>();
  for (const item of timeline) {
    if (item.eventType === "tool_use" && item.callId) uses.set(item.callId, item);
  }
  const result: ToolUi[] = [];
  for (const item of timeline) {
    if (item.eventType !== "tool_result" || !item.callId || !item.output) continue;
    const use = uses.get(item.callId);
    if (!use || itemKind(use) !== "shell") continue;
    const command = toolCommand(use);
    if (!isMultaraCommand(command)) continue;
    const text = item.output.trim();
    if (!text.startsWith("{") || !text.includes("\"_ui\"")) continue;
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      continue;
    }
    const ui = (parsed as { _ui?: unknown })?._ui;
    if (!ui) continue;
    const { specs } = parseSpecs(JSON.stringify(ui));
    const spec = specs?.[0];
    if (!spec || spec.type === "question") continue;
    result.push({ callId: item.callId, command, spec, raw: text });
  }
  return result;
}
