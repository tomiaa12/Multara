import type { Workspace } from "../../domain/types";

export type WorkspaceMode = "worktree" | "direct" | "folder";

export function workspaceMode(workspace: Workspace): WorkspaceMode {
  if (workspace.kind !== "git") return "folder";
  return workspace.isolationMode === "worktree" ? "worktree" : "direct";
}

export function workspaceModeLabel(workspace: Workspace): string {
  const mode = workspaceMode(workspace);
  if (mode === "worktree") return "Worktree";
  if (mode === "direct") return "当前分支";
  return "非 Git";
}

export function workspaceModeHint(workspace: Workspace): string {
  const mode = workspaceMode(workspace);
  if (mode === "worktree") return "Git Worktree：独立检出，不会改动原仓库当前分支";
  if (mode === "direct") return "直接在 Git 当前真实分支上修改，提交会写回原仓库";
  return workspace.isolationMode === "copy"
    ? "普通文件夹的隔离副本，不是 Git 项目"
    : "普通文件夹，直接使用原目录，不是 Git 项目";
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
  const branchText = branch ? `（分支 ${branch}）` : "";
  const lines: string[] = [];
  if (input.mode === "folder") {
    lines.push(
      input.isolationMode === "copy"
        ? `从这条需求移除「${input.name}」。这是隔离副本，不会删除原始文件夹，副本目录也会留在磁盘上，只解除关联。`
        : `从这条需求移除「${input.name}」。这是原目录，磁盘上的文件会保留，只是不再挂在这条需求上。`,
    );
  } else if (input.mode === "worktree") {
    if (input.sharedCheckout) {
      lines.push(
        `从这条需求移除 Worktree「${input.name}」${branchText}。这个工作树目录还被其他地方使用，这次只解除关联，不会删除目录，也不会动原仓库。`,
      );
    } else if (input.dirtyCount === null) {
      lines.push(
        `从这条需求移除 Worktree「${input.name}」${branchText}。独立工作树目录会被删除，原仓库不会动，分支会留在原仓库里。还没核对到未提交更改，如果工作树里有未提交代码，删除后会丢失。`,
      );
    } else if (input.dirtyCount > 0) {
      lines.push(
        `Worktree「${input.name}」${branchText}还有 ${input.dirtyCount} 个未提交更改。移除会删除这个独立工作树目录，这些未提交改动会丢失。原仓库不会被删除，已经提交的分支会保留。`,
      );
    } else {
      lines.push(
        `从这条需求移除 Worktree「${input.name}」${branchText}。工作树是干净的。独立工作树目录会被删除，原仓库不会动，这个分支会留在原仓库里。`,
      );
    }
  } else if (input.isolationMode === "copy") {
    lines.push(
      `从这条需求移除「${input.name}」${branchText}。不会删除原始代码文件夹，副本目录也会留在磁盘上，只解除关联。`,
    );
  } else if (input.dirtyCount === null) {
    lines.push(
      `从这条需求移除「${input.name}」${branchText}。这是真实仓库的当前分支，不会删除磁盘上的代码。还没核对到未提交更改，文件都会留在原处。`,
    );
  } else if (input.dirtyCount > 0) {
    lines.push(
      `「${input.name}」${branchText}正在直接使用真实分支，还有 ${input.dirtyCount} 个未提交更改。移除只会从这条需求拿掉，磁盘上的代码和这些未提交改动都会保留。`,
    );
  } else {
    lines.push(
      `从这条需求移除「${input.name}」${branchText}。这是真实仓库的当前分支，没有未提交更改。不会删除磁盘上的代码，只解除关联。`,
    );
  }
  if (input.unsavedCount > 0) {
    lines.push(
      `编辑器里还有 ${input.unsavedCount} 个未保存文件，移除后这些标签会关掉，未保存内容会丢失。`,
    );
  }
  lines.push("确定移除？");
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
  const branch = liveBranch || workspace.branch;
  if (mode === "folder") {
    return workspace.isolationMode === "copy" ? "普通文件夹 · 隔离副本" : "普通文件夹 · 原目录";
  }
  if (mode === "worktree") return `${branch || "HEAD"} · 独立工作树`;
  return `${branch || "HEAD"} · 正在改真实分支`;
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
