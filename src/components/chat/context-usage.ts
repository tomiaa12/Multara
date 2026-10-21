import type { ComputedRef, InjectionKey } from "vue";
import type { ChatMessage } from "../../domain/types";

/** 没有模型回传的 token 计数时，按约 4 个字符 1 token、20 万窗口估算。 */
export const CONTEXT_WINDOW_TOKENS = 200_000;
export const CHARS_PER_TOKEN = 4;

export const chatMessagesKey: InjectionKey<ComputedRef<ChatMessage[]>> = Symbol("chatMessages");

export interface ContextSlice {
  id: string;
  label: string;
  tokens: number;
  color: string;
}

export interface ContextUsage {
  used: number;
  window: number;
  percent: number;
  slices: ContextSlice[];
}

export function formatTokenCount(tokens: number): string {
  if (tokens >= 1000) return `${(tokens / 1000).toFixed(1)}K`;
  return String(tokens);
}

export function estimateContextUsage(messages: ChatMessage[]): ContextUsage {
  let conversationChars = 0;
  let attachmentBytes = 0;
  for (const message of messages) {
    conversationChars += message.content.length;
    for (const item of message.timeline ?? []) {
      conversationChars += item.content?.length ?? 0;
      conversationChars += item.output?.length ?? 0;
    }
    for (const attachment of message.attachments ?? []) {
      attachmentBytes += attachment.size;
      conversationChars += attachment.name.length;
    }
  }
  const conversation = Math.ceil(conversationChars / CHARS_PER_TOKEN);
  const attachments = Math.ceil(attachmentBytes / CHARS_PER_TOKEN);
  const slices: ContextSlice[] = [
    { id: "conversation", label: "对话", tokens: conversation, color: "#4c9dff" },
  ];
  if (attachments > 0) {
    slices.push({ id: "attachments", label: "附件", tokens: attachments, color: "#c084fc" });
  }
  const used = conversation + attachments;
  const percent = (used / CONTEXT_WINDOW_TOKENS) * 100;
  return { used, window: CONTEXT_WINDOW_TOKENS, percent, slices };
}
