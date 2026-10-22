import type { McpProbe } from "../domain/types";

/** 列表副标题：已检测则显示「N 个工具」，否则引导查看。 */
export function mcpToolsCaption(input: {
  probe?: McpProbe | null;
  loading?: boolean;
  enabled?: boolean;
}): string {
  if (input.enabled === false) return "已禁用";
  if (input.loading) return "加载中…";
  const probe = input.probe;
  if (!probe) return "查看工具";
  if (probe.status === "needs_auth") return "需要授权";
  if (probe.error && !probe.tools.length) return "检测失败";
  return `${probe.tools.length} 个工具`;
}
