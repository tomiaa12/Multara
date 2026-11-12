import { toRaw, type ComputedRef, type InjectionKey } from "vue";
import type { ChatMessage } from "../../domain/types";
import { t } from "../../i18n";

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

const SLICE_COLORS = {
  conversation: "#c026d3",
  attachments: "#a855f7",
  input: "#4c9dff",
  output: "#f472b6",
  cacheRead: "#34d399",
  cacheWrite: "#fbbf24",
} as const;

export function formatTokenCount(tokens: number): string {
  if (tokens >= 1000) return `${(tokens / 1000).toFixed(1)}K`;
  return String(tokens);
}

/** 本对话里出过场的智能体（有助手回复或带 agentId 的消息）。 */
export function listContextAgentIds(messages: ChatMessage[]): string[] {
  const ids: string[] = [];
  const seen = new Set<string>();
  for (const message of messages) {
    const id = message.agentId?.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    ids.push(id);
  }
  return ids;
}

/** 全部，或某个智能体相关的消息（其回复 + 发给它的用户消息）。 */
export function messagesForContextAgent(
  messages: ChatMessage[],
  agentId: string | null,
): ChatMessage[] {
  if (!agentId) return messages;
  const turnIds = new Set(
    messages
      .filter((item) => item.agentId === agentId && item.turnId)
      .map((item) => item.turnId as string),
  );
  return messages.filter((message) => {
    if (message.agentId === agentId) return true;
    if (message.role === "user" && message.targetType === "agent" && message.targetId === agentId) {
      return true;
    }
    if (message.role === "user" && message.turnId && turnIds.has(message.turnId)) return true;
    return false;
  });
}

type TimelineItem = NonNullable<ChatMessage["timeline"]>[number];

// 流式更新整项替换时间线条目（见 stores/app.ts），所以按条目对象缓存字符数，只有新换上的那条需要重算。
const timelineItemChars = new WeakMap<TimelineItem, number>();

function itemChars(item: TimelineItem): number {
  const raw = toRaw(item);
  const cached = timelineItemChars.get(raw);
  if (cached !== undefined) return cached;
  const chars = (raw.content?.length ?? 0) + (raw.output?.length ?? 0);
  timelineItemChars.set(raw, chars);
  return chars;
}

function usageTotal(usage: NonNullable<ChatMessage["usage"]>): number {
  return usage.inputTokens
    + usage.outputTokens
    + usage.cacheReadTokens
    + usage.cacheWriteTokens;
}

function pushSlice(slices: ContextSlice[], id: string, label: string, tokens: number, color: string) {
  if (tokens <= 0) return;
  slices.push({ id, label, tokens, color });
}

export function estimateContextUsage(
  messages: ChatMessage[],
  agentId: string | null = null,
): ContextUsage {
  const scoped = messagesForContextAgent(messages, agentId);
  let conversationChars = 0;
  let attachmentBytes = 0;
  let inputTokens = 0;
  let outputTokens = 0;
  let cacheReadTokens = 0;
  let cacheWriteTokens = 0;
  let reported = 0;

  for (const message of scoped) {
    const usage = message.usage;
    if (usage && usageTotal(usage) > 0) {
      inputTokens += usage.inputTokens;
      outputTokens += usage.outputTokens;
      cacheReadTokens += usage.cacheReadTokens;
      cacheWriteTokens += usage.cacheWriteTokens;
      reported += usageTotal(usage);
      continue;
    }
    conversationChars += message.content.length;
    for (const item of message.timeline ?? []) conversationChars += itemChars(item);
    for (const attachment of message.attachments ?? []) {
      attachmentBytes += attachment.size;
      conversationChars += attachment.name.length;
    }
  }

  const conversation = Math.ceil(conversationChars / CHARS_PER_TOKEN);
  const attachments = Math.ceil(attachmentBytes / CHARS_PER_TOKEN);
  const slices: ContextSlice[] = [];

  if (reported > 0) {
    pushSlice(slices, "input", t("chat.usage.slice.input"), inputTokens, SLICE_COLORS.input);
    pushSlice(slices, "output", t("chat.usage.slice.output"), outputTokens, SLICE_COLORS.output);
    pushSlice(slices, "cacheRead", t("chat.usage.slice.cacheRead"), cacheReadTokens, SLICE_COLORS.cacheRead);
    pushSlice(slices, "cacheWrite", t("chat.usage.slice.cacheWrite"), cacheWriteTokens, SLICE_COLORS.cacheWrite);
  }
  pushSlice(slices, "conversation", t("chat.usage.slice.conversation"), conversation, SLICE_COLORS.conversation);
  pushSlice(slices, "attachments", t("chat.usage.slice.attachments"), attachments, SLICE_COLORS.attachments);

  const used = reported + conversation + attachments;
  const percent = (used / CONTEXT_WINDOW_TOKENS) * 100;
  return { used, window: CONTEXT_WINDOW_TOKENS, percent, slices };
}
