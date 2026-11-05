import { t } from "../../i18n";
import { computeLineDiff } from "../../utils/diff-engine";
import { formatRelativeTime } from "../ui/relative-time";

export interface GitBlameInfo {
  committed: boolean;
  hash: string;
  authorName: string;
  authorEmail: string;
  authorTime: number;
  subject: string;
  /** 这个提交之前最后一次改动该文件的提交；没有则为空串。 */
  previous?: string;
}

export interface BlameDiffLine {
  kind: "deleted" | "added";
  text: string;
}

const SUBJECT_MAX = 48;

export function blameAgo(unixSeconds: number, nowMs: number): string {
  if (!unixSeconds) return formatRelativeTime(nowMs, nowMs);
  return formatRelativeTime(unixSeconds * 1000, nowMs);
}

export function blameAbsoluteTime(unixSeconds: number, nowMs: number): string {
  const ms = unixSeconds ? unixSeconds * 1000 : nowMs;
  return new Date(ms).toLocaleString("zh-CN", {
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "numeric",
    minute: "numeric",
  });
}

export function blameAuthorLabel(info: GitBlameInfo): string {
  const name = info.authorName.trim();
  if (name) return name;
  return info.committed ? "?" : t("scm.blame.you");
}

export function blameSubjectLabel(info: GitBlameInfo): string {
  if (!info.committed) return t("scm.blame.uncommitted");
  return info.subject.trim() || info.hash.slice(0, 7);
}

/** 行尾装饰：作者, N前 • subject（GitLens currentLine 风）。 */
export function blameEditorText(info: GitBlameInfo, nowMs: number): string {
  const author = blameAuthorLabel(info);
  const ago = blameAgo(info.authorTime, nowMs);
  const subject = truncate(blameSubjectLabel(info), SUBJECT_MAX);
  return `${author}, ${ago} • ${subject}`;
}

/** 状态栏：作者 (相对时间)。 */
export function blameStatusText(info: GitBlameInfo, nowMs: number): string {
  return `${blameAuthorLabel(info)} (${blameAgo(info.authorTime, nowMs)})`;
}

export function blameStatusTitle(info: GitBlameInfo, nowMs: number): string {
  return `${blameAuthorLabel(info)}, ${blameAgo(info.authorTime, nowMs)} (${blameAbsoluteTime(info.authorTime, nowMs)})\n${blameSubjectLabel(info)}`;
}

/** 只抽指定新行这一行的红/绿 diff（同 GitLens hovers.changesDiff=line）。 */
export function blameLineDiffSnippet(
  oldText: string,
  newText: string,
  newLine: number,
): BlameDiffLine[] {
  if (oldText === newText) {
    const line = lineAt(newText, newLine);
    return line == null ? [] : [{ kind: "added", text: line }];
  }
  const hunks = computeLineDiff(oldText, newText);
  const hit = hunks.find((hunk) => {
    if (hunk.newLines.length === 0) {
      return newLine === hunk.newStart || newLine === hunk.newStart - 1;
    }
    const end = hunk.newStart + hunk.newLines.length - 1;
    return newLine >= hunk.newStart && newLine <= end;
  });
  if (!hit) {
    const line = lineAt(newText, newLine);
    return line == null ? [] : [{ kind: "added", text: line }];
  }
  if (hit.newLines.length === 0) {
    return hit.oldLines.map((text) => ({ kind: "deleted" as const, text }));
  }
  const index = newLine - hit.newStart;
  const added = hit.newLines[index];
  if (added == null) return [];
  const rows: BlameDiffLine[] = [];
  if (index < hit.oldLines.length) {
    rows.push({ kind: "deleted", text: hit.oldLines[index] });
  }
  rows.push({ kind: "added", text: added });
  return rows;
}

export function blameExplainPrompt(input: {
  filePath: string;
  subject: string;
  hash: string;
  diff: BlameDiffLine[];
}): string {
  const short = input.hash ? input.hash.slice(0, 7) : "";
  const header = short
    ? t("scm.blame.explainCommitted", { subject: input.subject, hash: short, path: input.filePath })
    : t("scm.blame.explainUncommitted", { path: input.filePath });
  const body = input.diff
    .map((row) => `${row.kind === "deleted" ? "-" : "+"}${row.text}`)
    .join("\n");
  return `${header}\n\`\`\`diff\n${body}\n\`\`\``;
}

export function remoteCommitWebUrl(remote: string, hash: string): string | null {
  const base = normalizeRemoteWebBase(remote);
  if (!base || !hash) return null;
  try {
    const host = new URL(base).hostname;
    if (host.includes("gitlab")) return `${base}/-/commit/${hash}`;
    if (host.includes("bitbucket")) return `${base}/commits/${hash}`;
    return `${base}/commit/${hash}`;
  } catch {
    return null;
  }
}

export function normalizeRemoteWebBase(remote: string): string | null {
  let url = remote.trim().replace(/\.git$/, "");
  const scp = /^(?:[\w.-]+@)?([\w.-]+):(?!\/\/)(.+)$/.exec(url);
  if (scp) url = `https://${scp[1]}/${scp[2]}`;
  url = url.replace(/^ssh:\/\/(?:[\w.-]+@)?([\w.-]+)(?::\d+)?\//, "https://$1/").replace(/^git:\/\//, "https://");
  url = url.replace(/^https?:\/\/[^@/]+@/, "https://");
  return /^https?:\/\//.test(url) ? url : null;
}

function lineAt(text: string, line: number): string | null {
  if (line < 1) return null;
  const lines = text.split("\n");
  return line <= lines.length ? lines[line - 1] : null;
}

function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1)}…`;
}
