import type { Workspace } from "../../domain/types";
import { t } from "../../i18n";

export type WorkspaceMode = "worktree" | "direct" | "folder";

export function workspaceMode(workspace: Workspace): WorkspaceMode {
  if (workspace.kind !== "git") return "folder";
  return workspace.isolationMode === "worktree" ? "worktree" : "direct";
}

export function workspaceModeLabel(workspace: Workspace): string {
  const mode = workspaceMode(workspace);
  if (mode === "worktree") return t("workspace.mode.worktree");
  if (mode === "direct") return t("workspace.mode.direct");
  return t("workspace.mode.folder");
}

export function workspaceModeHint(workspace: Workspace): string {
  const mode = workspaceMode(workspace);
  if (mode === "worktree") return t("workspace.modeHint.worktree");
  if (mode === "direct") return t("workspace.modeHint.direct");
  if (workspace.isolationMode === "copy") return t("workspace.modeHint.folderCopy");
  return t("workspace.modeHint.folderDirect");
}

/** 从需求里拿掉一个项目前的确认文案。非 Worktree 只解除关联，不删真实目录。 */
export function detachWorkspacePrompt(input: {
  name: string;
  mode: WorkspaceMode;
  isolationMode: Workspace["isolationMode"];
  branch: string;
  /** null 表示 Git 状态还没拉到，不能当成干净。 */
  dirtyCount: number | null;
  unsavedCount: number;
  /** 同一条工作树目录还挂在别的会话上时，不能删目录。 */
  sharedCheckout: boolean;
}): string {
  const branch = input.branch.trim();
  const branchText = branch ? t("workspace.detach.branchSuffix", { branch }) : "";
  const lines: string[] = [];
  if (input.mode === "folder") {
    lines.push(
      input.isolationMode === "copy"
        ? t("workspace.detach.folderCopy", { name: input.name })
        : t("workspace.detach.folderDirect", { name: input.name }),
    );
  } else if (input.mode === "worktree") {
    if (input.sharedCheckout) {
      lines.push(t("workspace.detach.worktreeShared", { name: input.name, branch: branchText }));
    } else if (input.dirtyCount === null) {
      lines.push(t("workspace.detach.worktreeUnknownDirty", { name: input.name, branch: branchText }));
    } else if (input.dirtyCount > 0) {
      lines.push(
        t("workspace.detach.worktreeDirty", { name: input.name, branch: branchText, count: input.dirtyCount }),
      );
    } else {
      lines.push(t("workspace.detach.worktreeClean", { name: input.name, branch: branchText }));
    }
  } else if (input.isolationMode === "copy") {
    lines.push(t("workspace.detach.gitCopy", { name: input.name, branch: branchText }));
  } else if (input.dirtyCount === null) {
    lines.push(t("workspace.detach.gitUnknownDirty", { name: input.name, branch: branchText }));
  } else if (input.dirtyCount > 0) {
    lines.push(
      t("workspace.detach.gitDirty", { name: input.name, branch: branchText, count: input.dirtyCount }),
    );
  } else {
    lines.push(t("workspace.detach.gitClean", { name: input.name, branch: branchText }));
  }
  if (input.unsavedCount > 0) {
    lines.push(t("workspace.detach.unsavedEditors", { count: input.unsavedCount }));
  }
  lines.push(t("workspace.detach.confirm"));
  return lines.join("\n\n");
}

export function workspaceBranchName(workspace: Workspace, liveBranch?: string | null): string {
  const live = liveBranch?.trim();
  if (live) return live;
  return workspace.branch?.trim() ?? "";
}

/** 这条需求里已经挂上的 Git 分支，按添加顺序，同名只留第一次。 */
export function requirementBranchNames(workspaces: Workspace[], projectId: string): string[] {
  const seen = new Set<string>();
  const names: string[] = [];
  for (const workspace of workspaces) {
    if (workspace.projectId !== projectId || workspace.kind !== "git") continue;
    const branch = workspace.branch?.trim();
    if (!branch || seen.has(branch)) continue;
    seen.add(branch);
    names.push(branch);
  }
  return names;
}

/** 把需求名收成合法分支后缀，例如「登录页改版」→「登录页改版」。 */
export function branchSlugFromName(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, "-")
    .replace(/[~^:?*[\\@{]+/g, "")
    .replace(/\.\.+/g, "")
    .replace(/\/+/g, "-")
    .replace(/[^\w\u4e00-\u9fff.+-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "")
    .replace(/\.lock$/i, "");
  return slug || "feature";
}

/** 常规提交风格的分支前缀。自定义前缀必须能通过 sanitizeBranchPrefix。 */
export const BRANCH_PREFIXES = [
  "feat",
  "fix",
  "docs",
  "style",
  "refactor",
  "perf",
  "test",
  "build",
  "ci",
  "chore",
  "revert",
] as const;

/** 前缀只保留字母、数字、点、下划线和连字符，避免拼出非法 Git 引用。 */
export function sanitizeBranchPrefix(prefix: string): string {
  const cleaned = prefix
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "")
    .replace(/^[-.]+|[-.]+$/g, "");
  return cleaned.slice(0, 32);
}

/** 根据需求名生成推荐分支名，例如 feat/登录页改版。前缀来自设置，默认 feat。 */
export function branchNameFromRequirement(requirementName: string, prefix = "feat"): string {
  const safePrefix = sanitizeBranchPrefix(prefix) || "feat";
  return `${safePrefix}/${branchSlugFromName(requirementName)}`;
}

export function defaultNewBranchName(
  requirementName: string,
  knownBranches: string[],
  prefix = "feat",
): string {
  return knownBranches[0] ?? branchNameFromRequirement(requirementName, prefix);
}

export function workspaceSubtitle(workspace: Workspace, liveBranch?: string | null): string {
  const mode = workspaceMode(workspace);
  const branch = liveBranch || workspace.branch || "HEAD";
  if (mode === "folder") {
    return workspace.isolationMode === "copy"
      ? t("workspace.subtitle.folderCopy")
      : t("workspace.subtitle.folderDirect");
  }
  if (mode === "worktree") return t("workspace.subtitle.worktree", { branch });
  return t("workspace.subtitle.direct", { branch });
}

export function fileNameOf(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function dirNameOf(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts.length > 1 ? parts.slice(0, -1).join("/") : "";
}

export function joinWorkPath(workPath: string, relative = ""): string {
  const root = workPath.replace(/[\\/]+$/, "");
  const next = relative.replace(/^[\\/]+/, "");
  if (!next) return root;
  return `${root}/${next}`;
}
