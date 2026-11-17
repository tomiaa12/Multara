import { describe, expect, test } from "vitest";
import { hasAwaitingQuestion, resolveQuestionStates } from "./question-state";

function msg(
  partial: Partial<{
    id: string;
    role: string;
    content: string;
    failureReason: string | null;
    messageKind: string | null;
    audienceType: string | null;
  }> & { id: string; role: string; content: string },
) {
  return {
    failureReason: null,
    messageKind: "message",
    audienceType: "user",
    ...partial,
  };
}

const questionContent = [
  "请确认。",
  "",
  "```multara-questions",
  JSON.stringify({
    questions: [{
      header: "确认创建",
      question: "按上面预览创建？",
      options: [{ label: "确认创建" }, { label: "先别创建" }],
    }],
  }),
  "```",
].join("\n");

describe("resolveQuestionStates", () => {
  test("marks answered when a later assignment follows an agent-audience question", () => {
    const messages = [
      msg({
        id: "q",
        role: "assistant",
        content: questionContent,
        audienceType: "agent",
      }),
      msg({
        id: "status",
        role: "assistant",
        content: "预览对上了，但还没建。",
        audienceType: "user",
      }),
      msg({
        id: "assign",
        role: "assistant",
        content: "按确认创建执行。",
        messageKind: "assignment",
        audienceType: "agent",
      }),
      msg({
        id: "done",
        role: "assistant",
        content: "建仓完成。",
        audienceType: "agent",
      }),
    ];
    expect(resolveQuestionStates(messages).get("q")).toBe("answered");
    expect(hasAwaitingQuestion(messages)).toBe(false);
  });

  test("does not treat agent-audience questions as user-pending", () => {
    const messages = [
      msg({
        id: "q",
        role: "assistant",
        content: questionContent,
        audienceType: "agent",
      }),
    ];
    expect(resolveQuestionStates(messages).has("q")).toBe(false);
    expect(hasAwaitingQuestion(messages)).toBe(false);
  });

  test("keeps user-facing questions pending until answered", () => {
    const messages = [
      msg({ id: "q", role: "assistant", content: questionContent, audienceType: "user" }),
    ];
    expect(resolveQuestionStates(messages).get("q")).toBe("pending");
    expect(hasAwaitingQuestion(messages)).toBe(true);
  });

  test("marks answered after a later user message", () => {
    const messages = [
      msg({ id: "q", role: "assistant", content: questionContent }),
      msg({ id: "a", role: "user", content: "确认创建" }),
    ];
    expect(resolveQuestionStates(messages).get("q")).toBe("answered");
  });

  test("marks skipped when dismissed and nothing resolved it", () => {
    const messages = [
      msg({ id: "q", role: "assistant", content: questionContent }),
    ];
    expect(resolveQuestionStates(messages, { dismissedIds: ["q"] }).get("q")).toBe("skipped");
  });
});
