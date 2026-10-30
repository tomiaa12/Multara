import type { FileEntry } from "../../../domain/types";
import { sortExplorerEntries, type SortOptions } from "./explorer-compare";
import { nestDirectoryFiles } from "./explorer-nesting";

export interface ExplorerEntryLike extends FileEntry {
  mtime?: number;
  size?: number;
  readonly?: boolean;
}

export interface ExplorerRow extends ExplorerEntryLike {
  workspaceId: string;
  depth: number;
  /** 紧凑文件夹：从链首到链尾的每一级路径与名称，`path` 是链尾。 */
  compact?: { paths: string[]; names: string[] };
  /** 文件嵌套：父文件可以展开，子文件缩进显示在它下面。 */
  nestParent?: boolean;
  nestedChild?: boolean;
  expandable: boolean;
  expanded: boolean;
  /** 磁盘上已不存在、只在待审 Diff 里的文件。 */
  missing?: boolean;
  /** 被 files.exclude/.gitignore 隐藏，但因为已在编辑器打开而强制显示。 */
  excluded?: boolean;
  /** 查找过滤命中的字符区间（相对显示名）。 */
  matches?: Array<[number, number]>;
}

export interface ExplorerRowNode {
  entry: ExplorerRow;
  expanded: boolean;
  children: ExplorerRowNode[];
}

export interface BuildRowsInput {
  workspaceId: string;
  workspaceName: string;
  roots: ExplorerEntryLike[] | undefined;
  childrenOf: (path: string) => ExplorerEntryLike[] | undefined;
  isExpanded: (path: string) => boolean;
  isNestExpanded: (path: string) => boolean;
  sort: SortOptions;
  compactFolders: boolean;
  nestingPatterns: Record<string, string> | null;
  /** 根据 files.exclude 等判断是否隐藏；`siblings` 是同目录全部名字。 */
  isHidden: (entry: ExplorerEntryLike, siblings: ReadonlySet<string>) => boolean;
  /** 隐藏项若已在编辑器打开（或含已打开的文件）仍显示。 */
  forceVisible: (path: string) => boolean;
  /** 紧凑链在这些路径处断开（例如正在新建或重命名的文件夹）。 */
  breakCompactAt: (path: string) => boolean;
  missingFiles: string[];
}

function parentOf(path: string) {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? "" : path.slice(0, slash);
}

function nameOf(path: string) {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? path : path.slice(slash + 1);
}

function visibleChildren(
  entries: ExplorerEntryLike[],
  isHidden: (entry: ExplorerEntryLike, siblings: ReadonlySet<string>) => boolean,
  forceVisible: (path: string) => boolean,
) {
  const names = new Set(entries.map((entry) => entry.name));
  const out: Array<ExplorerEntryLike & { excluded?: boolean }> = [];
  for (const entry of entries) {
    if (!isHidden(entry, names)) {
      out.push(entry);
    } else if (forceVisible(entry.path)) {
      out.push({ ...entry, excluded: true });
    }
  }
  return out;
}

/**
 * VS Code `autoExpandCompressedChildren`：可见子项恰好为一个目录时返回它，
 * 否则 null（压缩后需连同展开，否则只剩折叠的 `a/b` 行）。
 */
export function soleCompressibleChild(
  entries: ExplorerEntryLike[] | undefined,
  isHidden: (entry: ExplorerEntryLike, siblings: ReadonlySet<string>) => boolean,
  forceVisible: (path: string) => boolean = () => false,
): ExplorerEntryLike | null {
  if (!entries) return null;
  const children = visibleChildren(entries, isHidden, forceVisible);
  if (children.length !== 1 || !children[0].isDirectory) return null;
  return children[0];
}

function compactChain(input: BuildRowsInput, head: ExplorerEntryLike) {
  const chain: ExplorerEntryLike[] = [head];
  if (!input.compactFolders) return chain;
  let current = head;
  while (!input.breakCompactAt(current.path)) {
    const loaded = input.childrenOf(current.path);
    if (!loaded) break;
    const child = soleCompressibleChild(loaded, input.isHidden, input.forceVisible);
    if (!child) break;
    current = child;
    chain.push(current);
  }
  return chain;
}

function buildLevel(
  input: BuildRowsInput,
  entries: ExplorerEntryLike[],
  dirPath: string,
  depth: number,
): ExplorerRowNode[] {
  const visible = visibleChildren(entries, input.isHidden, input.forceVisible);
  let nests = new Map<string, string[]>();
  if (input.nestingPatterns) {
    const files = visible.filter((entry) => !entry.isDirectory).map((entry) => entry.name);
    nests = nestDirectoryFiles(files, nameOf(dirPath), input.nestingPatterns);
  }
  const nestedNames = new Set([...nests.values()].flat());
  const byName = new Map(visible.map((entry) => [entry.name, entry]));
  const top = visible
    .filter((entry) => entry.isDirectory || !nestedNames.has(entry.name) || nests.has(entry.name))
    .map((entry) => ({ ...entry, hasNests: nests.has(entry.name) }));

  const nodes: ExplorerRowNode[] = [];
  for (const entry of sortExplorerEntries(top, input.sort)) {
    if (entry.isDirectory) {
      const chain = compactChain(input, entry);
      const tail = chain[chain.length - 1];
      const expanded = input.isExpanded(tail.path);
      const row: ExplorerRow = {
        ...tail,
        workspaceId: input.workspaceId,
        depth,
        expandable: true,
        expanded,
        excluded: (entry as { excluded?: boolean }).excluded,
        ...(chain.length > 1
          ? { name: chain.map((item) => item.name).join("/"), compact: { paths: chain.map((item) => item.path), names: chain.map((item) => item.name) } }
          : {}),
      };
      const loaded = input.childrenOf(tail.path);
      nodes.push({
        entry: row,
        expanded,
        children: expanded && loaded ? buildLevel(input, loaded, tail.path, depth + 1) : [],
      });
      continue;
    }
    const nested = nests.get(entry.name);
    if (nested && nested.length) {
      const expanded = input.isNestExpanded(entry.path);
      const children = sortExplorerEntries(
        nested.map((name) => byName.get(name)).filter((item): item is ExplorerEntryLike => Boolean(item)),
        input.sort,
      ).map<ExplorerRowNode>((child) => ({
        entry: { ...child, workspaceId: input.workspaceId, depth: depth + 1, expandable: false, expanded: false, nestedChild: true },
        expanded: false,
        children: [],
      }));
      nodes.push({
        entry: { ...entry, workspaceId: input.workspaceId, depth, expandable: true, expanded, nestParent: true },
        expanded,
        children: expanded ? children : [],
      });
      continue;
    }
    nodes.push({
      entry: { ...entry, workspaceId: input.workspaceId, depth, expandable: false, expanded: false },
      expanded: false,
      children: [],
    });
  }
  return nodes;
}

function insertMissing(nodes: ExplorerRowNode[], input: BuildRowsInput) {
  const seen = new Set<string>();
  const walk = (list: ExplorerRowNode[]) => {
    for (const node of list) {
      seen.add(node.entry.path);
      walk(node.children);
    }
  };
  walk(nodes);
  const findLevel = (dir: string): ExplorerRowNode[] | null => {
    if (!dir) return nodes;
    const stack = [...nodes];
    while (stack.length) {
      const node = stack.pop()!;
      if (node.entry.isDirectory && node.entry.path === dir) return node.expanded ? node.children : null;
      stack.push(...node.children);
    }
    return null;
  };
  for (const path of input.missingFiles) {
    if (seen.has(path)) continue;
    const parent = parentOf(path);
    const level = findLevel(parent);
    if (!level) continue;
    const depth = path.split("/").filter(Boolean).length - 1;
    level.push({
      entry: {
        name: nameOf(path),
        path,
        isDirectory: false,
        depth: Math.max(0, depth),
        workspaceId: input.workspaceId,
        expandable: false,
        expanded: false,
        missing: true,
      },
      expanded: false,
      children: [],
    });
  }
}

export function buildExplorerRows(input: BuildRowsInput): ExplorerRowNode[] {
  const nodes = buildLevel(input, input.roots ?? [], "", 0);
  insertMissing(nodes, input);
  return nodes;
}

export function flattenRows(nodes: ExplorerRowNode[], out: ExplorerRow[] = []): ExplorerRow[] {
  for (const node of nodes) {
    out.push(node.entry);
    if (node.expanded) flattenRows(node.children, out);
  }
  return out;
}

/**
 * 模糊匹配：查询里的字符按顺序出现在名字里即可（忽略大小写），优先连续命中。
 * 返回命中区间，未命中返回 null。
 */
export function fuzzyMatch(query: string, name: string): Array<[number, number]> | null {
  const q = query.toLowerCase();
  const n = name.toLowerCase();
  if (!q) return [];
  const contiguous = n.indexOf(q);
  if (contiguous !== -1) return [[contiguous, contiguous + q.length]];
  const ranges: Array<[number, number]> = [];
  let from = 0;
  for (const char of q) {
    const index = n.indexOf(char, from);
    if (index === -1) return null;
    const last = ranges[ranges.length - 1];
    if (last && last[1] === index) last[1] = index + 1;
    else ranges.push([index, index + 1]);
    from = index + 1;
  }
  return ranges;
}

/** 过滤模式：只保留命中项及其祖先；高亮模式：保留全部，只标出命中。 */
export function applyFind(
  nodes: ExplorerRowNode[],
  query: string,
  mode: "filter" | "highlight",
): ExplorerRowNode[] {
  if (!query) return nodes;
  const visit = (list: ExplorerRowNode[]): ExplorerRowNode[] => {
    const out: ExplorerRowNode[] = [];
    for (const node of list) {
      const matches = fuzzyMatch(query, node.entry.name);
      const children = visit(node.children);
      const entry = matches ? { ...node.entry, matches } : node.entry;
      if (mode === "highlight" || matches || children.length) {
        out.push({ ...node, entry, children });
      }
    }
    return out;
  };
  return visit(nodes);
}
