import type { GlobalResource, Workspace } from "../domain/types";
import type { SideNavGroup, SideNavItem } from "../components/SideNav.vue";

export const ALL_SOURCES = "all";
export const MULTARA_OWNER = "multara";

/** 来源列表排序：Multara 固定紧跟「全部」，其余按中文 label。 */
export function compareResourceSourceLabels(
  left: { id: string; label: string },
  right: { id: string; label: string },
): number {
  if (left.id === MULTARA_OWNER && right.id !== MULTARA_OWNER) return -1;
  if (right.id === MULTARA_OWNER && left.id !== MULTARA_OWNER) return 1;
  return left.label.localeCompare(right.label, "zh");
}

/** 工作区 skill 路径 key → 展示名 */
export const WORKSPACE_SKILL_PATHS: Record<string, string> = {
  agents: ".agents/skills",
  claude: ".claude/skills",
  github: ".github/skills",
  cursor: ".cursor/skills",
};

/** 工作区 MCP 路径 key → 展示名 */
export const WORKSPACE_MCP_PATHS: Record<string, string> = {
  mcp: ".mcp.json",
  cursor: ".cursor/mcp.json",
  vscode: ".vscode/mcp.json",
};

export function parseWorkspaceOwner(owner: string): { workspaceId: string; key: string } | null {
  if (!owner.startsWith("ws/")) return null;
  const rest = owner.slice(3);
  const slash = rest.indexOf("/");
  if (slash <= 0 || slash === rest.length - 1) return null;
  return { workspaceId: rest.slice(0, slash), key: rest.slice(slash + 1) };
}

/** 项目级筛选 id：`ws/<workspaceId>`；路径级：`ws/<workspaceId>/<key>` */
export function matchesResourceSource(owner: string, source: string): boolean {
  if (source === ALL_SOURCES) return true;
  const parsed = parseWorkspaceOwner(owner);
  if (source.startsWith("ws/") && !parseWorkspaceOwner(source)) {
    // `ws/<id>`：选中该工作区下全部路径
    return Boolean(parsed && owner.startsWith(`${source}/`));
  }
  return owner === source;
}

export function isSourceIdKnown(source: string, knownIds: Iterable<string>): boolean {
  for (const id of knownIds) {
    if (id === source) return true;
  }
  return false;
}

type PathBucket = { owner: string; key: string; label: string; count: number };

function workspaceLabel(workspaceId: string, workspaces: Workspace[], fallback: string) {
  return workspaces.find((item) => item.id === workspaceId)?.name
    || fallback
    || workspaceId;
}

function pathLabel(kind: "skill" | "mcp", key: string, sourceLabel: string) {
  const map = kind === "skill" ? WORKSPACE_SKILL_PATHS : WORKSPACE_MCP_PATHS;
  if (map[key]) return map[key];
  const fromLabel = sourceLabel.split(" · ").at(-1)?.trim();
  return fromLabel || key;
}

export function buildResourceSourceNav(input: {
  resources: GlobalResource[];
  workspaces: Workspace[];
  kind: "skill" | "mcp";
  activeSource: string;
  globalLabel: (owner: string) => string;
}): { groups: SideNavGroup[]; knownIds: string[] } {
  const globalCounts = new Map<string, number>();
  const projects = new Map<string, { name: string; paths: Map<string, PathBucket> }>();

  for (const resource of input.resources) {
    const owner = resource.owner || "multara";
    const parsed = parseWorkspaceOwner(owner);
    if (!parsed) {
      globalCounts.set(owner, (globalCounts.get(owner) ?? 0) + 1);
      continue;
    }
    const project = projects.get(parsed.workspaceId) ?? {
      name: workspaceLabel(parsed.workspaceId, input.workspaces, resource.sourceLabel.split(" · ")[0] ?? ""),
      paths: new Map(),
    };
    const path = project.paths.get(parsed.key) ?? {
      owner,
      key: parsed.key,
      label: pathLabel(input.kind, parsed.key, resource.sourceLabel),
      count: 0,
    };
    path.count += 1;
    project.paths.set(parsed.key, path);
    projects.set(parsed.workspaceId, project);
  }

  const knownIds = [ALL_SOURCES];
  const globalItems: SideNavItem[] = [{
    id: ALL_SOURCES,
    label: "全部",
    badge: input.resources.length,
    active: input.activeSource === ALL_SOURCES,
  }];
  for (const [owner, count] of [...globalCounts.entries()].sort((left, right) =>
    compareResourceSourceLabels(
      { id: left[0], label: input.globalLabel(left[0]) },
      { id: right[0], label: input.globalLabel(right[0]) },
    ),
  )) {
    knownIds.push(owner);
    globalItems.push({
      id: owner,
      label: input.globalLabel(owner),
      badge: count,
      active: input.activeSource === owner,
    });
  }

  const groups: SideNavGroup[] = [{ label: "来源", items: globalItems }];
  if (!projects.size) return { groups, knownIds };

  const projectItems: SideNavItem[] = [...projects.entries()]
    .map(([workspaceId, project]) => {
      const projectId = `ws/${workspaceId}`;
      knownIds.push(projectId);
      const children = [...project.paths.values()]
        .sort((left, right) => left.label.localeCompare(right.label, "zh"))
        .map((path) => {
          knownIds.push(path.owner);
          return {
            id: path.owner,
            label: path.label,
            badge: path.count,
            active: input.activeSource === path.owner,
          } satisfies SideNavItem;
        });
      const total = children.reduce((sum, item) => sum + Number(item.badge ?? 0), 0);
      return {
        id: projectId,
        label: project.name,
        badge: total,
        active: input.activeSource === projectId,
        children,
      } satisfies SideNavItem;
    })
    .sort((left, right) => left.label.localeCompare(right.label, "zh"));

  groups.push({ label: "项目", items: projectItems });
  return { groups, knownIds };
}
