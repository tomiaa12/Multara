import raw from "../../shared/cli-catalog.json";

export type CliProtocol = "anthropic" | "openai" | "gemini";
export type CliUrlSuffix = "v1" | "plain";

export type CliCatalogEntry = {
  id: string;
  label: string;
  /**
   * ProviderLogo / 展示用图标 id；默认等于 `id`。
   * 例如 qoderclicn 可复用 qoder 的图。
   */
  icon?: string;
  command?: string;
  protocol?: CliProtocol;
  gateway?: boolean;
  urlSuffix?: CliUrlSuffix;
  /** 可作为「添加运行时」的基础 CLI */
  runtime?: boolean;
  /** 相对用户主目录的 skills 根路径，如 `.cursor/skills` */
  skillsHome?: string;
  /** 相对用户主目录的 MCP 配置路径 */
  mcpHome?: string;
  /** 特殊 MCP 路径解析（如 VS Code 用户目录） */
  mcpSpecial?: "vscode";
  supported?: boolean;
};

type CatalogFile = { entries: CliCatalogEntry[] };

const catalog = raw as CatalogFile;

export const CLI_CATALOG: CliCatalogEntry[] = catalog.entries;

const byId = new Map(CLI_CATALOG.map((item) => [item.id, item]));

export function cliEntry(id: string): CliCatalogEntry | undefined {
  return byId.get(id);
}

export function cliLabel(id: string): string {
  return byId.get(normalizeOwnerId(id))?.label ?? id;
}

/** 展示用：`~/.cursor/skills` */
export function cliSkillsDisplay(id: string): string | undefined {
  const home = byId.get(normalizeOwnerId(id))?.skillsHome;
  return home ? `~/${home}` : undefined;
}

/** 展示用：`~/.cursor/mcp.json` */
export function cliMcpDisplay(id: string): string | undefined {
  const home = byId.get(normalizeOwnerId(id))?.mcpHome;
  return home ? `~/${home}` : undefined;
}

/** 工作区 owner `ws/<id>/<key>` 取 key；其余原样。 */
export function normalizeOwnerId(owner: string): string {
  if (!owner.startsWith("ws/")) return owner;
  return owner.split("/").pop() || owner;
}

/** 给 ProviderLogo / CliSourceIcon 用的图标 id（来自公共配置 `icon`，默认即目录 id）。 */
export function cliIconProvider(owner: string): string {
  if (owner === "all") return "all";
  const id = normalizeOwnerId(owner);
  return byId.get(id)?.icon ?? id;
}

export function runtimeEntries(): CliCatalogEntry[] {
  return CLI_CATALOG.filter((item) => item.runtime && item.command && item.protocol);
}

export function skillSourceEntries(): CliCatalogEntry[] {
  return CLI_CATALOG.filter((item) => Boolean(item.skillsHome));
}
