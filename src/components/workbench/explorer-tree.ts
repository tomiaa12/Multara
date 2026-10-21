import type { FileEntry } from "../../domain/types";

export interface ExplorerTreeNode {
  entry: FileEntry;
  expanded: boolean;
  children: ExplorerTreeNode[];
}

export function nestExplorerRows(
  entries: FileEntry[],
  isExpanded: (entry: FileEntry) => boolean,
): ExplorerTreeNode[] {
  const nodes = new Map<string, ExplorerTreeNode>();
  const roots: ExplorerTreeNode[] = [];
  for (const entry of entries) {
    nodes.set(entry.path, {
      entry,
      expanded: isExpanded(entry),
      children: [],
    });
  }
  for (const entry of entries) {
    const node = nodes.get(entry.path);
    if (!node) continue;
    const slash = entry.path.lastIndexOf("/");
    const parent = slash === -1 ? "" : entry.path.slice(0, slash);
    const parentNode = parent ? nodes.get(parent) : undefined;
    if (parentNode?.entry.isDirectory) parentNode.children.push(node);
    else roots.push(node);
  }
  return roots;
}
