import { formatRelativeTime } from "../ui/relative-time";

export const BLAME_UNCOMMITTED = "尚未提交";

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

export const BLAME_CMD_OPEN_CHANGES = "multara.blame.openChanges";
export const BLAME_CMD_COPY_HASH = "multara.blame.copyHash";

function commandLink(label: string, command: string, args: unknown[], tooltip: string): string {
  return `[${label}](command:${command}?${encodeURIComponent(JSON.stringify(args))} "${tooltip}")`;
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

/** 对齐 VSCode getCommitHover：作者 + 相对时间(完整日期)，提交信息，分隔线，短哈希。 */
export function blameHoverText(info: GitBlameInfo, nowMs: number, ownerId?: number): string {
  const when = new Date(info.authorTime * 1000).toLocaleString("zh-CN", {
    year: "numeric", month: "long", day: "numeric", hour: "numeric", minute: "numeric",
  });
  const who = info.authorEmail ? `[**${info.authorName}**](mailto:${info.authorEmail})` : `**${info.authorName}**`;
  const message = info.subject.replace(/!\[/g, "&#33;&#91;");
  const short = info.hash.slice(0, 7);
  const parts = [
    `${who}, ${blameAgo(info.authorTime, nowMs)} (${when})`,
    message,
    "---",
  ];
  if (ownerId === undefined) {
    parts.push(`\`${short}\``);
    return parts.join("\n\n");
  }
  const actions = [
    `\`${short}\` ${commandLink("复制", BLAME_CMD_COPY_HASH, [ownerId], "复制提交哈希")}`,
    commandLink(
      `更改 ${info.previous ? info.previous.slice(0, 7) : "∅"} ⟷ ${short}`,
      BLAME_CMD_OPEN_CHANGES,
      [ownerId],
      "打开此提交对该文件的更改",
    ),
  ];
  parts.push(actions.join("&nbsp;&nbsp;|&nbsp;&nbsp;"));
  return parts.join("\n\n");
}
