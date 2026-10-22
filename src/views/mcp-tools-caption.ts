import type { McpProbe } from "../domain/types";
import { t } from "../i18n";

/** 列表副标题：已检测则显示「N 个工具」，否则引导查看。 */
export function mcpToolsCaption(input: {
  probe?: McpProbe | null;
  loading?: boolean;
  enabled?: boolean;
}): string {
  if (input.enabled === false) return t("mcp.status.disabled");
  if (input.loading) return t("mcp.caption.loading");
  const probe = input.probe;
  if (!probe) return t("mcp.caption.viewTools");
  if (probe.status === "needs_auth") return t("mcp.status.needsAuth");
  if (probe.error && !probe.tools.length) return t("mcp.caption.probeFailed");
  return t("mcp.caption.toolCount", { count: probe.tools.length });
}
