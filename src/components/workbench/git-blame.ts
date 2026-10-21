import { formatRelativeTime } from "../ui/relative-time";

export const BLAME_UNCOMMITTED = "尚未提交";

export interface GitBlameInfo {
  committed: boolean;
  hash: string;
  authorName: string;
  authorEmail: string;
  authorTime: number;
  subject: string;
}

export function blameAgo(unixSeconds: number, nowMs: number): string {
  return formatRelativeTime(unixSeconds * 1000, nowMs);
}

/** 行尾装饰，对应 git.blame.editorDecoration.template 的默认值。 */
export function blameEditorText(info: GitBlameInfo, nowMs: number): string {
  return `${info.subject}, ${info.authorName} (${blameAgo(info.authorTime, nowMs)})`;
}

/** 状态栏，对应 git.blame.statusBarItem.template 的默认值。 */
export function blameStatusText(info: GitBlameInfo, nowMs: number): string {
  return `${info.authorName} (${blameAgo(info.authorTime, nowMs)})`;
}

export function blameHoverText(info: GitBlameInfo, nowMs: number): string {
  const when = new Date(info.authorTime * 1000).toLocaleString("zh-CN", { hour12: false });
  const who = info.authorEmail ? `${info.authorName} <${info.authorEmail}>` : info.authorName;
  return [info.subject, "", `\`${info.hash.slice(0, 7)}\` · ${who}`, "", when, "", blameAgo(info.authorTime, nowMs)].join("\n");
}
