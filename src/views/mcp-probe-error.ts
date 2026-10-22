/** 把 MCP 探测原始错误收成可读文案的 i18n key；对不上则返回 null，调用方用原文。 */
export function mcpProbeErrorKey(raw: string): string | null {
  const text = raw.trim();
  if (!text) return "mcp.error.generic";
  if (/授权|401|403|auth|unauthorized|forbidden/i.test(text)) return "mcp.error.auth";
  if (/超时|timeout/i.test(text)) return "mcp.error.timeout";
  if (/找不到命令|No such file or directory|os error 2|not found|ENOENT/i.test(text)) {
    if (/\bnpx\b|\bnode\b/i.test(text)) return "mcp.error.npxMissing";
    return "mcp.error.commandMissing";
  }
  if (/无法启动|failed to (start|spawn)|spawn/i.test(text)) return "mcp.error.startFailed";
  return null;
}
