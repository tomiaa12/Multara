import type { Workspace, WorkspaceCandidate } from "../../domain/types";
import type { WorkspaceAddRequest } from "../../stores/app";
import {
  defaultNewBranchName,
  requirementBranchNames,
} from "../workbench/workspace-mode";

export type GitMode = "worktree-new" | "worktree-existing" | "current";

export type PickerItem = WorkspaceCandidate & {
  gitMode: GitMode;
  baseRef: string;
  checkoutName: string;
};

export type BoundWorkspace = { path: string; git: boolean };

export function normalizeBindPath(path: string) {
  return path.replace(/^\\\\\?\\/i, "").replaceAll("\\", "/").replace(/\/+$/, "").toLowerCase();
}

export function isAlreadyBoundPath(path: string, bound: BoundWorkspace[]) {
  const normalized = normalizeBindPath(path);
  return bound.some((item) => {
    if (normalized === item.path) return true;
    return item.git && normalized.startsWith(`${item.path}/`);
  });
}

export function boundWorkspacesOf(workspaces: Workspace[], projectId: string): BoundWorkspace[] {
  return workspaces
    .filter((workspace) => workspace.projectId === projectId)
    .map((workspace) => ({
      path: normalizeBindPath(workspace.sourcePath),
      git: workspace.kind === "git",
    }));
}

export function knownBranchesOf(workspaces: Workspace[], projectId: string) {
  return requirementBranchNames(workspaces, projectId);
}

export function createPickerItem(
  candidate: WorkspaceCandidate,
  requirementName: string,
  knownBranches: string[],
  branchPrefix: string,
): PickerItem {
  const name = requirementName.trim() || candidate.name;
  return {
    ...candidate,
    gitMode: "worktree-new",
    baseRef: candidate.currentBranch ?? candidate.branches[0] ?? "HEAD",
    checkoutName:
      candidate.kind === "git"
        ? defaultNewBranchName(name, knownBranches, branchPrefix)
        : "",
  };
}

export function validatePicker(
  items: PickerItem[],
  bound: BoundWorkspace[],
): { code: "already" | "branch" | "empty"; name?: string } | null {
  const already = items.filter((item) => isAlreadyBoundPath(item.path, bound));
  if (already.length) return { code: "already" };
  const missingName = items.find(
    (item) =>
      !isAlreadyBoundPath(item.path, bound) &&
      item.kind === "git" &&
      item.gitMode === "worktree-new" &&
      !item.checkoutName.trim(),
  );
  if (missingName) return { code: "branch", name: missingName.name };
  if (!items.some((item) => !isAlreadyBoundPath(item.path, bound))) return { code: "empty" };
  return null;
}

export function buildWorkspaceRequests(
  items: PickerItem[],
  bound: BoundWorkspace[],
): WorkspaceAddRequest[] {
  return items
    .filter((item) => !isAlreadyBoundPath(item.path, bound))
    .map((item) => ({
      name: item.name,
      path: item.path,
      mode:
        item.kind !== "git" || item.gitMode === "current"
          ? ("direct" as const)
          : ("isolated" as const),
      baseRef: item.gitMode === "worktree-new" ? item.baseRef : item.checkoutName,
      checkoutName: item.checkoutName.trim(),
      createBranch: item.gitMode === "worktree-new",
    }));
}
