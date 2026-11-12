import { COUNT_BADGE_LIMIT, formatCountBadge } from "../ui/count-badge";

export type InboxNoticeInput = {
  status: string;
  previousStatus?: string;
  sessionId: string;
  projectId: string | null;
  taskId: string | null;
  currentPath: string;
  /** 收件箱里当前打开的会话；正在看则不再当未读通知。 */
  activeInboxSessionId?: string | null;
  /** 右下角气泡是否打开，且当前会话就是这条。 */
  chatOpen?: boolean;
  activeSessionId?: string | null;
};

/**
 * 运行从「运行中」进入结束态时，是否当作未读（并弹系统通知）。
 * 收件箱仍会写入这条会话；只有当时不在对应界面（任务详情 / 需求工作台 / 气泡该会话 / 收件箱该会话）才标未读。
 * 自动化会话由后端直接写入收件箱，这里跳过。
 */
export function shouldNotifyInbox(input: InboxNoticeInput): boolean {
  const finished = input.status === "completed"
    || input.status === "failed"
    || input.status === "stopped"
    || input.status === "needs_input"
    || input.status === "needs_permission";
  if (!finished || input.previousStatus !== "running") return false;
  if (input.sessionId.startsWith("automation-run:")) return false;
  if (
    input.currentPath === "/inbox"
    && input.activeInboxSessionId
    && input.activeInboxSessionId === input.sessionId
  ) {
    return false;
  }
  if (input.taskId) return input.currentPath !== `/tasks/${input.taskId}`;
  if (input.projectId) return input.currentPath !== `/projects/${input.projectId}`;
  // 右下角气泡 / 无需求会话：正在看这条就不标未读
  if (input.chatOpen && input.activeSessionId === input.sessionId) return false;
  return true;
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
