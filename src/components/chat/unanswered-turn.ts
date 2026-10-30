import type { ChatMessage, MessageAttachmentInput } from "../../domain/types";

/** 助手是否已经产出过任何可见内容（正文 / 时间线 / 失败 / 空响应占位）。 */
export function assistantHasOutput(message: ChatMessage): boolean {
  if (message.role === "user") return false;
  if (message.failureReason) return true;
  if (message.messageKind === "no_response") return true;
  if (message.content?.trim()) return true;
  if (message.timeline?.length) return true;
  return false;
}

/**
 * 若最后一轮用户提问之后还没有任何助手输出，返回该用户消息；否则 null。
 * 用于「尚未开始回答时停止 → 撤回提问并回填输入框」。
 */
export function lastUnansweredUser(messages: readonly ChatMessage[]): ChatMessage | null {
  let lastUser: ChatMessage | null = null;
  for (const message of messages) {
    if (message.role === "user") {
      lastUser = message;
      continue;
    }
    if (!lastUser) continue;
    if (assistantHasOutput(message)) lastUser = null;
  }
  return lastUser;
}

export function attachmentsAsInput(message: ChatMessage): MessageAttachmentInput[] {
  return message.attachments.map(({ name, path, mimeType, size }) => ({
    name,
    path,
    mimeType,
    size,
  }));
}
