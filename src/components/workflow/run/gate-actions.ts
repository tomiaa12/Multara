import { scanFences } from "../../chat/genui/block";

export interface GateAction {
  id: string;
  label: string;
}

/** 上一步回复末尾的 ```multara-actions 代码块。取最后一块。 */
export function gateActionsFromText(text: string): GateAction[] {
  const blocks = scanFences(text, "multara-actions").filter((segment) => segment.kind === "block");
  const last = blocks[blocks.length - 1];
  if (!last || last.kind !== "block") return [];
  try {
    const parsed = JSON.parse(last.raw) as { actions?: unknown };
    if (!Array.isArray(parsed.actions)) return [];
    return parsed.actions.flatMap((item) => {
      if (!item || typeof item !== "object") return [];
      const record = item as { id?: unknown; label?: unknown };
      const id = String(record.id ?? "").trim();
      const label = String(record.label ?? "").trim();
      if (!id || !label) return [];
      return [{ id, label }];
    });
  } catch {
    return [];
  }
}

/** 展示正文时去掉按钮代码块，按钮单独渲染。 */
export function stripGateActions(text: string): string {
  return scanFences(text, "multara-actions")
    .flatMap((segment) => (segment.kind === "text" ? [segment.text] : []))
    .join("\n")
    .trim();
}
