import type { GitFileStatus } from "../../domain/types";
import { t } from "../../i18n";
import { gitStatusLetter, gitStatusLetterFor } from "./tree-git-status";

export type ScmGroupId = "merge" | "staged" | "changes" | "untracked";

export const SCM_GROUP_ORDER: ScmGroupId[] = ["merge", "staged", "changes", "untracked"];

export function scmGroupLabel(id: ScmGroupId): string {
  if (id === "merge") return t("scm.group.merge");
  if (id === "staged") return t("scm.group.staged");
  if (id === "untracked") return t("scm.group.untracked");
  return t("scm.group.changes");
}

/** @deprecated use scmGroupLabel — kept for call sites that expect a map */
export const SCM_GROUP_LABEL: Record<ScmGroupId, string> = {
  get merge() { return scmGroupLabel("merge"); },
  get staged() { return scmGroupLabel("staged"); },
  get changes() { return scmGroupLabel("changes"); },
  get untracked() { return scmGroupLabel("untracked"); },
};

export type ScmOpenTarget = {
  workspaceId: string;
  path: string;
  group: ScmGroupId | "commit" | "local";
  commit?: string;
  parent?: string | null;
  status?: string;
  localId?: string;
};

export type ScmTreeNode =
  | {
      kind: "dir";
      path: string;
      name: string;
      count: number;
      children: ScmTreeNode[];
    }
  | {
      kind: "file";
      path: string;
      file: GitFileStatus;
    };

function isMergeConflict(file: GitFileStatus) {
  const index = file.indexStatus;
  const worktree = file.worktreeStatus;
  if (index === "U" || worktree === "U") return true;
  return (index === "A" && worktree === "A") || (index === "D" && worktree === "D");
}

export function scmGroups(changes: GitFileStatus[]): Record<ScmGroupId, GitFileStatus[]> {
  const groups: Record<ScmGroupId, GitFileStatus[]> = {
    merge: [],
    staged: [],
    changes: [],
    untracked: [],
  };
  const sorted = [...changes].sort((a, b) => a.path.localeCompare(b.path));
  for (const file of sorted) {
    if (isMergeConflict(file)) {
      groups.merge.push(file);
      continue;
    }
    if (file.indexStatus === "?" || file.worktreeStatus === "?") {
      groups.untracked.push(file);
      continue;
    }
    if (file.indexStatus !== " ") groups.staged.push(file);
    if (file.worktreeStatus !== " ") groups.changes.push(file);
  }
  return groups;
}

export function scmResourceLetter(file: GitFileStatus, group: ScmGroupId) {
  if (group === "merge") return gitStatusLetter(file) || "C";
  if (group === "staged") return gitStatusLetterFor(file, "index") || "M";
  return gitStatusLetterFor(file, "worktree") || "M";
}

export function scmFileTree(files: GitFileStatus[]): ScmTreeNode[] {
  const root: Extract<ScmTreeNode, { kind: "dir" }> = {
    kind: "dir",
    path: "",
    name: "",
    count: 0,
    children: [],
  };
  for (const file of [...files].sort((a, b) => a.path.localeCompare(b.path))) {
    const parts = file.path.split("/").filter(Boolean);
    let cursor = root;
    for (let index = 0; index < parts.length - 1; index += 1) {
      const path = parts.slice(0, index + 1).join("/");
      let child = cursor.children.find((item) => item.kind === "dir" && item.path === path);
      if (!child) {
        child = { kind: "dir", path, name: parts[index], count: 0, children: [] };
        cursor.children.push(child);
      }
      if (child.kind === "dir") {
        child.count += 1;
        cursor = child;
      }
    }
    const name = parts[parts.length - 1] ?? file.path;
    cursor.children.push({ kind: "file", path: file.path, file: { ...file, path: file.path || name } });
  }
  const sortNodes = (nodes: ScmTreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
      const aName = a.kind === "dir" ? a.name : a.path;
      const bName = b.kind === "dir" ? b.name : b.path;
      return aName.localeCompare(bName);
    });
    for (const node of nodes) {
      if (node.kind === "dir") sortNodes(node.children);
    }
  };
  sortNodes(root.children);
  return root.children;
}
