import { extractQuestions } from "./question-block";

export type QuestionResolveState = "pending" | "answered" | "skipped";

export type QuestionStateMessage = {
  id: string;
  role: string;
  content: string;
  failureReason?: string | null;
  messageKind?: string | null;
  audienceType?: string | null;
};

/**
 * 从后往前扫：后面已有用户回复或队长 assignment，提问算已回答；
 * 智能体互问（audience=agent）不占用用户输入框上方的 pending。
 */
export function resolveQuestionStates(
  messages: QuestionStateMessage[],
  options: { genui?: boolean; dismissedIds?: Iterable<string> } = {},
): Map<string, QuestionResolveState> {
  const states = new Map<string, QuestionResolveState>();
  const dismissed = new Set(options.dismissedIds ?? []);
  let resolvedAfter = false;
  let pendingTaken = false;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role === "user") {
      resolvedAfter = true;
      continue;
    }
    if (message.messageKind === "assignment") resolvedAfter = true;
    if (message.role !== "assistant" || message.failureReason) continue;
    if (!extractQuestions(message.content, { genui: options.genui }).questions.length) continue;
    if (resolvedAfter) {
      states.set(message.id, "answered");
      continue;
    }
    if (dismissed.has(message.id)) {
      states.set(message.id, "skipped");
      continue;
    }
    // 问的是另一个智能体（如队长），等对方 assignment，不塞给用户答
    if (message.audienceType === "agent") continue;
    if (pendingTaken) continue;
    states.set(message.id, "pending");
    pendingTaken = true;
  }
  return states;
}

/** 会话是否还在等用户回答提问（会挡住非优先排队消息） */
export function hasAwaitingQuestion(
  messages: QuestionStateMessage[],
  options: { genui?: boolean; dismissedIds?: Iterable<string> } = {},
): boolean {
  for (const state of resolveQuestionStates(messages, options).values()) {
    if (state === "pending") return true;
  }
  return false;
}
