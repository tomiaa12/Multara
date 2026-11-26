export type CleanupMarker = "worktree" | "branch" | "folder";

export function cleanupMarker(workspace: { kind: string; isolationMode: string }): CleanupMarker {
  if (workspace.isolationMode === "worktree") return "worktree";
  if (workspace.kind === "git") return "branch";
  return "folder";
}

export function hasConflict(status: {
  merging: boolean;
  changes: { indexStatus: string; worktreeStatus: string }[];
} | undefined) {
  if (!status) return false;
  if (status.merging) return true;
  return status.changes.some(
    (file) =>
      file.indexStatus === "U"
      || file.worktreeStatus === "U"
      || (file.indexStatus === "A" && file.worktreeStatus === "A")
      || (file.indexStatus === "D" && file.worktreeStatus === "D"),
  );
}

/** 只默认勾选已提交、且不在解决冲突的 worktree。 */
export function defaultCleanupChecked(input: {
  marker: CleanupMarker;
  sessionBound: boolean;
  missing: boolean;
  dirty: boolean;
  conflict: boolean;
}) {
  return (
    input.marker === "worktree"
    && !input.sessionBound
    && !input.missing
    && !input.dirty
    && !input.conflict
  );
}

/** 上游可能带远程名。对得上本地分支才预选，对不上交给用户自己选。 */
export function matchPreferredBranch(branches: string[], preferred: string) {
  const name = preferred.trim();
  if (!name) return null;
  if (branches.includes(name)) return name;
  const slash = name.indexOf("/");
  if (slash > 0) {
    const rest = name.slice(slash + 1);
    if (rest && branches.includes(rest)) return rest;
  }
  return null;
}

export function formatDiskSize(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 KB";
  const kb = 1024;
  const mb = kb * 1024;
  const gb = mb * 1024;
  if (bytes < kb) return `${Math.round(bytes)} B`;
  if (bytes < mb) return `${trimSize(bytes / kb)} KB`;
  if (bytes < gb) return `${trimSize(bytes / mb)} MB`;
  return `${trimSize(bytes / gb)} GB`;
}

function trimSize(value: number) {
  const digits = value >= 10 ? 0 : 1;
  return value.toFixed(digits).replace(/\.0$/, "");
}
