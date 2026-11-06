import type { FileEntry, GitHistoryItem, GitLineBlame } from "../../domain/types";
import * as backend from "../../services/backend";
import type { TreemapInput } from "./squarify";

export type VisualHistoryScope = "current" | "repo";
export type TreemapMode = "files" | "commits";

export interface InspectHistoryRow {
  hash: string;
  subject: string;
  author: string;
  committedAt: string;
  parents: string[];
}

export interface TimelinePoint {
  hash: string;
  subject: string;
  author: string;
  at: number;
  /** 该提交改动文件数，用于可视化历史气泡大小。 */
  weight: number;
  refs: string[];
}

export type HistoryListMode = "commits" | "contributors";

export interface InspectContributorRow {
  author: string;
  commits: number;
  lastAt: string;
}

const LINE_HISTORY_LIMIT = 40;
/** 提交矩形图采样上限（再按 period 过滤）。过大时 gitCommitFiles 会拖死首屏。 */
const COMMIT_SAMPLE = 80;

export function historyToInspectRows(items: GitHistoryItem[]): InspectHistoryRow[] {
  return items.map((item) => ({
    hash: item.hash,
    subject: item.subject || item.hash.slice(0, 7),
    author: item.author,
    committedAt: item.committedAt,
    parents: item.parents,
  }));
}

/** 沿 blame.previous 回溯当前行改动提交（对齐 GitLens Line History 的近似数据源）。 */
export async function loadLineHistory(
  workspaceId: string,
  path: string,
  line: number,
): Promise<InspectHistoryRow[]> {
  if (!workspaceId || !path || line < 1) return [];
  const rows: InspectHistoryRow[] = [];
  const seen = new Set<string>();
  let revision: string | undefined;
  for (let i = 0; i < LINE_HISTORY_LIMIT; i += 1) {
    let blame: GitLineBlame | null = null;
    try {
      blame = await backend.gitBlameLine(workspaceId, path, line, revision);
    } catch {
      break;
    }
    if (!blame?.committed || !blame.hash || seen.has(blame.hash)) break;
    seen.add(blame.hash);
    rows.push({
      hash: blame.hash,
      subject: blame.subject || blame.hash.slice(0, 7),
      author: blame.authorName || "?",
      committedAt: blame.authorTime ? new Date(blame.authorTime * 1000).toISOString() : "",
      parents: blame.previous ? [blame.previous] : [],
    });
    if (!blame.previous) break;
    revision = blame.previous;
  }
  return rows;
}

export function historyToTimelinePoints(items: GitHistoryItem[]): TimelinePoint[] {
  return items
    .map((item) => ({
      hash: item.hash,
      subject: item.subject || item.hash.slice(0, 7),
      author: item.author,
      at: Date.parse(item.committedAt) || 0,
      weight: 1,
      refs: item.refs ?? [],
    }))
    .filter((item) => item.at > 0)
    .sort((a, b) => a.at - b.at);
}

/** 为时间线点补上改动文件数，驱动气泡半径。 */
export async function attachTimelineWeights(
  workspaceId: string,
  points: TimelinePoint[],
): Promise<TimelinePoint[]> {
  if (!workspaceId || !points.length) return points;
  const next = points.map((point) => ({ ...point }));
  await Promise.all(
    next.map(async (point) => {
      try {
        const files = await backend.gitCommitFiles(workspaceId, point.hash);
        point.weight = Math.max(1, files.length);
      } catch {
        point.weight = 1;
      }
    }),
  );
  return next;
}

export function contributorsFromHistory(rows: InspectHistoryRow[]): InspectContributorRow[] {
  const map = new Map<string, InspectContributorRow>();
  for (const row of rows) {
    const hit = map.get(row.author);
    if (!hit) {
      map.set(row.author, { author: row.author || "?", commits: 1, lastAt: row.committedAt });
      continue;
    }
    hit.commits += 1;
    if (row.committedAt > hit.lastAt) hit.lastAt = row.committedAt;
  }
  return [...map.values()].sort((a, b) => b.commits - a.commits || b.lastAt.localeCompare(a.lastAt));
}

export function countTreemapFiles(node: TreemapInput | null | undefined): number {
  if (!node) return 0;
  if (node.type === "file") return 1;
  let total = 0;
  for (const child of node.children ?? []) total += countTreemapFiles(child);
  return total;
}

/** 找到从根子节点到 target 的路径（不含根），对齐 GitLens findPath。 */
export function findTreemapPath(root: TreemapInput, target: TreemapInput): TreemapInput[] {
  const path: TreemapInput[] = [];
  const walk = (current: TreemapInput): boolean => {
    if (current === target || (current.path && current.path === target.path && current.type === target.type)) {
      return true;
    }
    for (const child of current.children ?? []) {
      if (walk(child)) {
        path.unshift(child);
        return true;
      }
    }
    return false;
  };
  walk(root);
  return path;
}

/** 时间跨度文案：几天 / 几个月 / 几年。 */
export function formatTimelineSpan(fromMs: number, toMs: number): string {
  const days = Math.max(1, Math.round((toMs - fromMs) / 86_400_000));
  if (days < 45) return `${days}d`;
  const months = Math.max(1, Math.round(days / 30));
  if (months < 18) return `${months}mo`;
  return `${Math.max(1, Math.round(months / 12))}y`;
}

/** GitLens 式气泡半径：按 weight 做 log 归一化。 */
export function timelineBubbleRadius(weight: number, maxWeight: number, minR = 3, maxR = 10): number {
  if (maxWeight <= 1) return (minR + maxR) / 2;
  const t = Math.min(1, Math.log1p(Math.max(1, weight)) / Math.log1p(maxWeight));
  return minR + (maxR - minR) * Math.sqrt(t);
}

export function buildTreemapFromEntries(
  entries: FileEntry[],
  frequencies?: Map<string, number>,
): TreemapInput {
  type Node = TreemapInput & { children: Node[] };
  const root: Node = { name: "", path: "", type: "folder", children: [] };
  const folders = new Map<string, Node>([["", root]]);

  function ensureFolder(dir: string): Node {
    const hit = folders.get(dir);
    if (hit) return hit;
    const parentPath = dir.includes("/") ? dir.slice(0, dir.lastIndexOf("/")) : "";
    const parent = ensureFolder(parentPath);
    const name = dir.includes("/") ? dir.slice(dir.lastIndexOf("/") + 1) : dir;
    const node: Node = { name, path: dir, type: "folder", children: [] };
    parent.children.push(node);
    folders.set(dir, node);
    return node;
  }

  for (const entry of entries) {
    if (entry.isDirectory) {
      ensureFolder(entry.path);
      continue;
    }
    const parentPath = entry.path.includes("/") ? entry.path.slice(0, entry.path.lastIndexOf("/")) : "";
    const parent = ensureFolder(parentPath);
    const freq = frequencies?.get(entry.path) ?? 0;
    const size = Math.max(1, entry.size ?? 1);
    const leaf: Node & { valueHint?: number } = {
      name: entry.name,
      path: entry.path,
      type: "file",
      children: [],
      valueHint: frequencies ? Math.max(1, freq) : size,
    };
    parent.children.push(leaf);
  }

  return root;
}

/** 给 squarify 用的叶子面积：挂在节点上的 valueHint，缺省 1。 */
export function treemapLeafValue(node: TreemapInput): number {
  const hint = (node as TreemapInput & { valueHint?: number }).valueHint;
  return Math.max(1, hint ?? 1);
}

/**
 * 采样提交改动文件得到路径频率（提交矩形图热度）。
 * `sinceMs` 有值时只统计该时间戳之后的提交（对齐 GitLens period 窗口）。
 */
export async function loadCommitFrequencies(
  workspaceId: string,
  sinceMs?: number | null,
): Promise<Map<string, number>> {
  const freq = new Map<string, number>();
  if (!workspaceId) return freq;
  let page;
  try {
    page = await backend.gitLog(workspaceId, 0, COMMIT_SAMPLE);
  } catch {
    return freq;
  }
  const items = sinceMs == null
    ? page.items
    : page.items.filter((item) => {
      const at = Date.parse(item.committedAt);
      return Number.isFinite(at) && at >= sinceMs;
    });
  await Promise.all(
    items.map(async (item) => {
      try {
        const files = await backend.gitCommitFiles(workspaceId, item.hash);
        for (const file of files) {
          freq.set(file.path, (freq.get(file.path) ?? 0) + 1);
        }
      } catch {
        /* ignore single commit */
      }
    }),
  );
  return freq;
}

/** 按仓库 / 当前路径过滤树条目。 */
export function filterEntriesByScope(
  entries: FileEntry[],
  scope: VisualHistoryScope,
  path: string,
): FileEntry[] {
  if (scope === "repo" || !path) return entries;
  return entries.filter((item) => item.path === path || item.path.startsWith(`${path}/`));
}

/** 扩展名 → 色相（对齐 GitLens getExtensionHue）。 */
export function extensionHue(pathOrName: string): number {
  const base = pathOrName.includes("/") ? pathOrName.slice(pathOrName.lastIndexOf("/") + 1) : pathOrName;
  const dot = base.lastIndexOf(".");
  if (dot < 0) return 0;
  const ext = base.slice(dot + 1).toLowerCase();
  const map: Record<string, number> = {
    ts: 210, tsx: 210, js: 50, jsx: 50, vue: 150, css: 300, scss: 300,
    html: 15, json: 140, md: 30, py: 60, go: 180, rs: 25, java: 200,
  };
  const known = map[ext];
  if (known != null) return known;
  let hash = 0;
  for (let i = 0; i < ext.length; i += 1) {
    hash = ((hash << 5) - hash + ext.charCodeAt(i)) | 0;
  }
  return ((hash % 360) + 360) % 360;
}
