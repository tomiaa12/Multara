import type { Diagnostic, NodeDef } from "../../../domain/workflow";

/** 诊断要落到的检查器控件。后端的 field 优先；没有时按错误码补上能确定的控件。 */
export function diagnosticField(item: Diagnostic, node?: NodeDef | null): string | undefined {
  if (item.field) return item.field;
  if (item.code === "agent_missing" || item.code === "squad_missing") return "config:target";
  if (item.code === "config_invalid" && node?.kind === "agent") {
    const prompt = (node.config as { prompt?: string } | undefined)?.prompt ?? "";
    if (!String(prompt).trim()) return "config:prompt";
  }
  return undefined;
}
