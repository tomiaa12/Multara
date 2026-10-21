export type ScmPrimaryAction = "commit" | "publish" | "sync";

export function canPublishBranch(repository: {
  hasRemote: boolean;
  upstream: string | null;
  merging: boolean;
}): boolean {
  return Boolean(repository.hasRemote && !repository.upstream && !repository.merging);
}

export function scmPrimaryAction(
  repository: {
    hasRemote: boolean;
    upstream: string | null;
    merging: boolean;
    ahead?: number;
    behind?: number;
  },
  canCommitNow = false,
): ScmPrimaryAction {
  if (canCommitNow) return "commit";
  if (canPublishBranch(repository)) return "publish";
  if (
    canSyncChanges({
      hasRemote: repository.hasRemote,
      upstream: repository.upstream,
      merging: repository.merging,
      ahead: repository.ahead ?? 0,
      behind: repository.behind ?? 0,
    })
  ) {
    return "sync";
  }
  return "commit";
}

export function scmBusyTitle(busy: string, own: string | string[], title: string) {
  const keys = Array.isArray(own) ? own : [own];
  if (busy && !keys.includes(busy)) return GIT_BUSY_TITLE;
  return title;
}

const commitMessageHistory = new Map<string, string[]>();
const commitMessageCursor = new Map<string, number>();

export function rememberCommitMessage(workspaceId: string, message: string) {
  const text = message.trim();
  if (!text) return;
  const previous = commitMessageHistory.get(workspaceId) ?? [];
  commitMessageHistory.set(
    workspaceId,
    [text, ...previous.filter((item) => item !== text)].slice(0, 50),
  );
  commitMessageCursor.set(workspaceId, -1);
}

export function stepCommitMessage(workspaceId: string, direction: "older" | "newer"): string | null {
  const list = commitMessageHistory.get(workspaceId) ?? [];
  if (!list.length) return null;
  let cursor = commitMessageCursor.get(workspaceId) ?? -1;
  if (direction === "older") {
    if (cursor + 1 >= list.length) return list[list.length - 1] ?? null;
    cursor += 1;
  } else if (cursor <= 0) {
    commitMessageCursor.set(workspaceId, -1);
    return "";
  } else {
    cursor -= 1;
  }
  commitMessageCursor.set(workspaceId, cursor);
  return list[cursor] ?? null;
}

export function resetCommitMessageCursor(workspaceId: string) {
  commitMessageCursor.set(workspaceId, -1);
}

export function canSyncChanges(repository: {
  hasRemote: boolean;
  upstream: string | null;
  merging: boolean;
  ahead: number;
  behind: number;
}): boolean {
  return Boolean(
    repository.hasRemote &&
      repository.upstream &&
      !repository.merging &&
      (repository.ahead > 0 || repository.behind > 0),
  );
}

export const GIT_BUSY_TITLE = "另一个工作区正在执行 Git 操作";
export const PUBLISH_STAGED_LEFT_NOTICE = "分支已发布。暂存的更改未提交，仍在本地。";

export function publishSkipsStagedChanges(message: string, stagedCount: number) {
  return stagedCount > 0 && !message.trim();
}

export function publishWithoutCommitConfirm(stagedCount: number) {
  return `暂存的 ${stagedCount} 个更改不会被提交，仍留在本地。仍要发布分支吗？`;
}

function discardName(path: string) {
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.split(/[\\/]/).pop() || trimmed || path;
}

export function discardConfirmMessage(
  files: Array<{ path: string; worktreeStatus: string }>,
): string | null {
  if (!files.length) return null;
  const untrackedDirs = files.filter(
    (file) => file.worktreeStatus === "?" && file.path.endsWith("/"),
  );
  const untrackedFiles = files.filter(
    (file) => file.worktreeStatus === "?" && !file.path.endsWith("/"),
  );
  const tracked = files.length - untrackedDirs.length - untrackedFiles.length;
  if (files.length === 1 && untrackedFiles.length === 1) {
    return `确定永久删除未跟踪文件 ${discardName(untrackedFiles[0].path)}？此操作不可撤销。`;
  }
  if (files.length === 1 && untrackedDirs.length === 1) {
    return `确定永久删除未跟踪目录 ${discardName(untrackedDirs[0].path)} 及其内部所有未跟踪文件？此操作不可撤销。`;
  }
  if (files.length === 1 && tracked === 1) {
    return `确定撤回 ${discardName(files[0].path)} 的本地更改？未保存的修改将丢失。`;
  }
  const parts: string[] = [];
  if (tracked) parts.push(`撤回 ${tracked} 个文件的本地更改`);
  if (untrackedFiles.length) parts.push(`永久删除 ${untrackedFiles.length} 个未跟踪文件`);
  if (untrackedDirs.length) {
    parts.push(`永久删除 ${untrackedDirs.length} 个未跟踪目录及其内部所有未跟踪文件`);
  }
  if (!parts.length) return null;
  return `确定${parts.join("，并")}？此操作不可撤销。`;
}
