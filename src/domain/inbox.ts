import { COUNT_BADGE_LIMIT, formatCountBadge } from "../ui/count-badge";

export type InboxNoticeInput = {
  status: string;
  previousStatus?: string;
  sessionId: string;
  projectId: string | null;
  taskId: string | null;
  currentPath: string;
};

/**
 * 只在一次运行从「运行中」进入结束态时通知。
 * 正在看这条需求的工作台，或正在看这个任务的详情时，结果已经在眼前，不再进收件箱。
 * 自动化会话由后端直接写入收件箱，这里不再走工作台聊天那条通知。
 */
export function shouldNotifyInbox(input: InboxNoticeInput): boolean {
  const finished = input.status === "completed"
    || input.status === "failed"
    || input.status === "stopped"
    || input.status === "needs_input"
    || input.status === "needs_permission";
  if (!finished || input.previousStatus !== "running") return false;
  if (input.sessionId.startsWith("automation-run:")) return false;
  if (input.taskId) return input.currentPath !== `/tasks/${input.taskId}`;
  if (input.projectId) return input.currentPath !== `/projects/${input.projectId}`;
  return false;
}

export function currentAppPath(): string {
  const hash = window.location.hash.replace(/^#/, "");
  const path = (hash.split("?")[0] || "/").trim();
  return path.startsWith("/") ? path : `/${path}`;
}

/** 侧栏和列表上的未读数，超过后显示 99+。 */
export const UNREAD_BADGE_LIMIT = COUNT_BADGE_LIMIT;

export function unreadBadgeLabel(count: number): string {
  return formatCountBadge(count);
}

export function unreadTotal(items: Array<{ unreadCount?: number }>): number {
  return items.reduce((sum, item) => sum + (item.unreadCount ?? 0), 0);
}

/**
 * 已读游标之后的第一条消息。没有游标时，整段对话都算未读。
 * 游标已经是最后一条时，没有新消息。
 */
export function firstUnreadMessageId(
  messageIds: readonly string[],
  readMessageId: string | null | undefined,
): string | null {
  if (!messageIds.length) return null;
  if (!readMessageId) return messageIds[0];
  const index = messageIds.indexOf(readMessageId);
  if (index < 0) return messageIds[0];
  return messageIds[index + 1] ?? null;
}

/** 列表只显示纯文本，去掉加粗标记。 */
export function plainInboxLine(source: string): string {
  return source.replace(/\*\*(.+?)\*\*/g, "$1").replace(/__(.+?)__/g, "$1").replace(/\*\*|__/g, "");
}

/** 收件箱列表用的一行摘要，优先取最后一条助手回复。 */
export function inboxPreview(assistantTexts: string[], fallback = ""): string {
  const source = [...assistantTexts].reverse().find((text) => text.trim()) ?? fallback;
  const text = plainInboxLine(source).replace(/\s+/g, " ").trim();
  if (!text) return "对话已结束";
  return text.length > 80 ? `${text.slice(0, 80)}…` : text;
}
