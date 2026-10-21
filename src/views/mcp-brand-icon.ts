/** 已知品牌：名称或 URL 命中时展示固定图标。 */
const BRAND_ICONS: Array<{ match: RegExp; icon: string }> = [
  { match: /feishu|lark|飞书|larksuite|larkoffice/, icon: "https://www.feishu.cn/favicon.ico" },
  { match: /figma/, icon: "https://static.figma.com/app/icon/1/favicon.png" },
  { match: /github/, icon: "https://github.githubassets.com/favicons/favicon.svg" },
  { match: /notion/, icon: "https://www.notion.so/images/favicon.ico" },
  { match: /linear\.app|\blinear\b/, icon: "https://linear.app/favicon.ico" },
  { match: /slack/, icon: "https://a.slack-edge.com/80588/marketing/img/meta/favicon-32.png" },
];

/** 无品牌图时的色块（稳定、偏中性，避免紫系）。 */
const FALLBACK_COLORS = [
  "#5B8C85",
  "#6B7C93",
  "#8B7355",
  "#7A8F6A",
  "#8A6E7A",
  "#6A8494",
  "#947A5E",
  "#5E7A8A",
];

/**
 * MCP 列表左侧品牌头像。
 * 优先按名称/摘要匹配已知品牌；HTTP MCP 再用主机 favicon。
 */
export function mcpBrandIconUrl(input: { name?: string | null; summary?: string | null }): string | undefined {
  const name = input.name?.trim() ?? "";
  const summary = input.summary?.trim() ?? "";
  const hay = `${name} ${summary}`.toLowerCase();
  for (const rule of BRAND_ICONS) {
    if (rule.match.test(hay)) return rule.icon;
  }
  const urlText = summary.match(/https?:\/\/\S+/)?.[0];
  if (!urlText) return undefined;
  try {
    const host = new URL(urlText).hostname;
    if (!host) return undefined;
    for (const rule of BRAND_ICONS) {
      if (rule.match.test(host)) return rule.icon;
    }
    return `https://${host}/favicon.ico`;
  } catch {
    return undefined;
  }
}

/** 无图标时的首字。 */
export function mcpBrandInitial(name?: string | null): string {
  const text = name?.trim() ?? "";
  if (!text) return "M";
  return text.slice(0, 1).toUpperCase();
}

/** 按名称生成稳定色块背景。 */
export function mcpBrandFallbackColor(name?: string | null): string {
  const text = name?.trim() ?? "";
  let hash = 0;
  for (let index = 0; index < text.length; index += 1) {
    hash = (hash * 31 + text.charCodeAt(index)) >>> 0;
  }
  return FALLBACK_COLORS[hash % FALLBACK_COLORS.length] ?? FALLBACK_COLORS[0];
}
