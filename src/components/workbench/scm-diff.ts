import { t } from "../../i18n";
import { languageFromFilePath } from "../../monaco-language";
import type { ScmGroupId, ScmOpenTarget } from "./scm-groups";
import { scmGroupLabel } from "./scm-groups";
import { fileNameOf } from "./workspace-mode";

export type ScmDiffPlan = {
  original: "empty" | "head" | "index" | { rev: string };
  modified: "empty" | "worktree" | "index" | { rev: string };
  label: string;
  short: string;
  /** 有值时编辑器 tab 主文案用它 */
  tabName?: string;
};

export type ScmDiffContent = {
  original: string;
  modified: string;
  binary: boolean;
  language: string;
  label: string;
  short: string;
  tabName?: string;
};

type RevisionText = { content: string; binary: boolean };

/** 工作区读文件：图片等媒体是 data URL，不能当文本 diff。 */
export type ScmWorktreeText = {
  content: string;
  language: string;
  binary?: boolean;
  media?: string;
};

const SHORT_HASH_LEN = 7;

function isMediaKind(media: string | undefined) {
  return media === "image" || media === "audio" || media === "video";
}

/** 工作区侧是否应按二进制处理（含媒体预览 data URL）。 */
export function isBinaryWorktree(file: Pick<ScmWorktreeText, "content" | "binary" | "media">) {
  if (file.binary) return true;
  if (isMediaKind(file.media?.trim())) return true;
  if (/^data:(?:image|audio|video)\//i.test(file.content)) return true;
  if (file.content.includes("\0")) return true;
  return false;
}

/** 扩展名已能判断是媒体时，不必再 readWorkspaceFile（大图 data URL 会卡死）。 */
const MEDIA_PATH_RE =
  /\.(?:png|jpe?g|jpe|gif|webp|bmp|ico|avif|svg|mp3|wav|ogg|m4a|aac|flac|mp4|webm|mov|m4v)$/i;

export function isMediaPath(path: string) {
  return MEDIA_PATH_RE.test(path);
}

function shortRev(rev: string) {
  return rev.slice(0, SHORT_HASH_LEN);
}

/** history diff 标题：name (parent) ↔ name (commit) */
export function scmCommitDiffTabName(input: {
  path: string;
  commit: string;
  parent: string | null | undefined;
  status?: string;
}) {
  const name = fileNameOf(input.path);
  const commitId = shortRev(input.commit);
  const parentId = input.parent ? shortRev(input.parent) : "";
  const added = input.status === "A" || input.status === "C" || !input.parent;
  const deleted = input.status === "D";
  if (added) return `${name} (${commitId})`;
  if (deleted) return `${name} (${parentId})`;
  return `${name} (${parentId}) \u2194 ${name} (${commitId})`;
}

/**
 * VS Code `resolveChangeCommand`：leftUri 为空时 `vscode.open` 文件本身，有左侧才 `vscode.diff`。
 * 未跟踪没有 HEAD/index；合并冲突打开带冲突标记的工作区文件。
 * 修改（changes）是 index ↔ 工作区，暂存（staged）是 HEAD ↔ index。
 */
export function scmOpensAsFile(group: ScmOpenTarget["group"]): boolean {
  return group === "untracked" || group === "merge";
}

export function scmDiffPlan(
  target: Pick<ScmOpenTarget, "group" | "commit" | "parent" | "status"> & { path?: string },
): ScmDiffPlan {
  if (target.group === "commit" && target.commit) {
    const added = target.status === "A" || target.status === "C";
    const deleted = target.status === "D";
    const tabName = scmCommitDiffTabName({
      path: target.path ?? "",
      commit: target.commit,
      parent: target.parent,
      status: target.status,
    });
    return {
      original: added || !target.parent ? "empty" : { rev: target.parent },
      modified: deleted ? "empty" : { rev: target.commit },
      label: tabName,
      short: "",
      tabName,
    };
  }
  if (target.group === "staged") {
    return {
      original: "head",
      modified: "index",
      label: scmGroupLabel("staged"),
      short: t("scm.diff.short.staged"),
    };
  }
  if (target.group === "untracked") {
    // 单击未跟踪项不会走到这里：VS Code 没有左侧版本，直接打开文件。保留计划给仍要做空 diff 的调用。
    return {
      original: "empty",
      modified: "worktree",
      label: t("scm.diff.untracked"),
      short: t("scm.diff.short.untracked"),
    };
  }
  const merge = target.group === "merge";
  if (merge) {
    return {
      original: "head",
      modified: "worktree",
      label: scmGroupLabel("merge"),
      short: t("scm.diff.short.merge"),
    };
  }
  return {
    original: "index",
    modified: "worktree",
    label: scmGroupLabel("changes"),
    short: t("scm.diff.short.changes"),
  };
}

type ScmDiffSide = ScmDiffPlan["original"] | ScmDiffPlan["modified"];

type ScmDiffIo = {
  readWorktree: (path: string) => Promise<ScmWorktreeText | null>;
  readIndex: (path: string) => Promise<RevisionText>;
  readRevision: (revision: string, path: string) => Promise<RevisionText>;
};

async function readSide(
  side: ScmDiffSide,
  path: string,
  io: ScmDiffIo,
): Promise<{ content: string; binary: boolean; language?: string }> {
  if (side === "empty") return { content: "", binary: false };
  if (side === "worktree") {
    const file = await io.readWorktree(path);
    if (!file) return { content: "", binary: false };
    const binary = isBinaryWorktree(file);
    return {
      content: binary ? "" : file.content,
      binary,
      language: file.language,
    };
  }
  if (side === "index") return io.readIndex(path);
  if (side === "head") return io.readRevision("HEAD", path);
  return io.readRevision(side.rev, path);
}

export async function loadScmDiff(
  target: ScmOpenTarget,
  io: ScmDiffIo,
): Promise<ScmDiffContent> {
  const plan = scmDiffPlan(target);
  const [original, modified] = await Promise.all([
    readSide(plan.original, target.path, io),
    readSide(plan.modified, target.path, io),
  ]);
  const binary = original.binary || modified.binary;
  return {
    // 任一侧是二进制/媒体时两侧都清空，避免 data URL 进 Monaco。
    original: binary ? "" : original.content,
    modified: binary ? "" : modified.content,
    binary,
    language: modified.language || original.language || languageFromFilePath(target.path),
    label: plan.label,
    short: plan.short,
    tabName: plan.tabName,
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
