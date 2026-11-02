import type { GitRepositoryStatus, Workspace } from "../../domain/types";
import { t } from "../../i18n";
import { syncAction, type SyncAction } from "../workbench/scm-branch";
import {
  workspaceMode,
  workspaceModeLabel,
  type WorkspaceMode,
} from "../workbench/workspace-mode";

export type WorkspaceDeleteChoice = {
  workspaceId: string;
  deleteLocalBranch: boolean;
  deleteRemoteBranch: boolean;
  deleteCopyDirectory: boolean;
};

export type WorkspaceDeleteRow = {
  workspace: Workspace;
  mode: WorkspaceMode;
  modeLabel: string;
  branch: string;
  protectedBranch: boolean;
  canDeleteLocal: boolean;
  canDeleteRemote: boolean;
  canDeleteCopy: boolean;
  hint: string;
  dirty: boolean;
  merging: boolean;
  sync: SyncAction | null;
  changeCount: number;
};

const PROTECTED_BRANCHES = new Set(["main", "master"]);

export function emptyChoice(workspaceId: string): WorkspaceDeleteChoice {
  return {
    workspaceId,
    deleteLocalBranch: false,
    deleteRemoteBranch: false,
    deleteCopyDirectory: false,
  };
}

export function isProtectedBranch(branch: string | null | undefined) {
  return Boolean(branch && PROTECTED_BRANCHES.has(branch));
}

export function deleteHint(workspace: Workspace) {
  const mode = workspaceMode(workspace);
  if (mode === "direct") return t("dashboard.deleteHint.direct");
  if (mode === "worktree") return t("dashboard.deleteHint.worktree");
  if (workspace.isolationMode === "copy") return t("dashboard.deleteHint.folderCopy");
  return t("dashboard.deleteHint.folderDirect");
}

export function buildWorkspaceDeleteRow(
  workspace: Workspace,
  status: GitRepositoryStatus | null | undefined,
): WorkspaceDeleteRow {
  const mode = workspaceMode(workspace);
  const branch = workspace.branch?.trim() || status?.branch?.trim() || "";
  const protectedBranch = isProtectedBranch(branch);
  return {
    workspace,
    mode,
    modeLabel: workspaceModeLabel(workspace),
    branch,
    protectedBranch,
    canDeleteLocal: mode === "worktree" && Boolean(branch) && !protectedBranch,
    canDeleteRemote:
      (mode === "worktree" || mode === "direct") && Boolean(branch) && !protectedBranch,
    canDeleteCopy: mode === "folder" && workspace.isolationMode === "copy",
    hint: deleteHint(workspace),
    dirty: Boolean(status?.changes?.length),
    merging: Boolean(status?.merging),
    sync: status ? syncAction(status) : null,
    changeCount: status?.changes?.length ?? 0,
  };
}

export function looksLikeConflict(message: string) {
  return /conflict|冲突|CONFLICT|未合并|merging/i.test(message);
}
