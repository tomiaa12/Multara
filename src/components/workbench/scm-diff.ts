import type { ScmGroupId, ScmOpenTarget } from "./scm-groups";

export type ScmDiffPlan = {
  original: "empty" | "head" | "index" | { rev: string };
  modified: "empty" | "worktree" | "index" | { rev: string };
  label: string;
  short: string;
};

export type ScmDiffContent = {
  original: string;
  modified: string;
  binary: boolean;
  language: string;
  label: string;
  short: string;
};

type RevisionText = { content: string; binary: boolean };

export function scmDiffPlan(target: Pick<ScmOpenTarget, "group" | "commit" | "parent" | "status">): ScmDiffPlan {
  if (target.group === "commit" && target.commit) {
    const added = target.status === "A" || target.status === "C";
    const deleted = target.status === "D";
    return {
      original: added || !target.parent ? "empty" : { rev: target.parent },
      modified: deleted ? "empty" : { rev: target.commit },
      label: "提交中的文件",
      short: "提交",
    };
  }
  if (target.group === "staged") {
    return { original: "head", modified: "index", label: "暂存的更改", short: "暂存" };
  }
  if (target.group === "untracked") {
    return { original: "empty", modified: "worktree", label: "未跟踪的更改", short: "未跟踪" };
  }
  const merge = target.group === "merge";
  if (merge) {
    return {
      original: "head",
      modified: "worktree",
      label: "合并更改",
      short: "合并",
    };
  }
  return {
    original: "index",
    modified: "worktree",
    label: "更改",
    short: "更改",
  };
}

function languageFromPath(path: string) {
  const lower = path.toLowerCase();
  if (lower.endsWith(".ts") || lower.endsWith(".tsx") || lower.endsWith(".mts")) return "typescript";
  if (lower.endsWith(".js") || lower.endsWith(".jsx") || lower.endsWith(".mjs")) return "javascript";
  if (lower.endsWith(".vue")) return "html";
  if (lower.endsWith(".json")) return "json";
  if (lower.endsWith(".md")) return "markdown";
  if (lower.endsWith(".css")) return "css";
  if (lower.endsWith(".rs")) return "rust";
  if (lower.endsWith(".py")) return "python";
  if (lower.endsWith(".html")) return "html";
  return "plaintext";
}

type ScmDiffSide = ScmDiffPlan["original"] | ScmDiffPlan["modified"];

async function readSide(
  side: ScmDiffSide,
  path: string,
  io: {
    readWorktree: (path: string) => Promise<{ content: string; language: string } | null>;
    readIndex: (path: string) => Promise<RevisionText>;
    readRevision: (revision: string, path: string) => Promise<RevisionText>;
  },
): Promise<{ content: string; binary: boolean; language?: string }> {
  if (side === "empty") return { content: "", binary: false };
  if (side === "worktree") {
    const file = await io.readWorktree(path);
    if (!file) return { content: "", binary: false };
    return { content: file.content, binary: false, language: file.language };
  }
  if (side === "index") return io.readIndex(path);
  if (side === "head") return io.readRevision("HEAD", path);
  return io.readRevision(side.rev, path);
}

export async function loadScmDiff(
  target: ScmOpenTarget,
  io: {
    readWorktree: (path: string) => Promise<{ content: string; language: string } | null>;
    readIndex: (path: string) => Promise<RevisionText>;
    readRevision: (revision: string, path: string) => Promise<RevisionText>;
  },
): Promise<ScmDiffContent> {
  const plan = scmDiffPlan(target);
  const [original, modified] = await Promise.all([
    readSide(plan.original, target.path, io),
    readSide(plan.modified, target.path, io),
  ]);
  return {
    original: original.binary ? "" : original.content,
    modified: modified.binary ? "" : modified.content,
    binary: original.binary || modified.binary,
    language: modified.language || original.language || languageFromPath(target.path),
    label: plan.label,
    short: plan.short,
  };
}

export function scmDiffId(target: ScmOpenTarget) {
  if (target.group === "local" && target.localId) {
    return `${target.workspaceId}:local:${target.localId}:${target.path}`;
  }
  if (target.group === "commit" && target.commit) {
    return `${target.workspaceId}:commit:${target.commit}:${target.path}`;
  }
  return `${target.workspaceId}:${target.group satisfies ScmGroupId | "commit" | "local"}:${target.path}`;
}
