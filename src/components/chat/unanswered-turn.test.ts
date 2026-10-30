import { describe, expect, test } from "vitest";
import type { ChatMessage } from "../../domain/types";
import { assistantHasOutput, lastUnansweredUser } from "./unanswered-turn";

function msg(partial: Partial<ChatMessage> & Pick<ChatMessage, "id" | "role">): ChatMessage {
  return {
    sessionId: "s",
    content: "",
    runId: null,
    agentId: null,
    audienceType: null,
    audienceId: null,
    targetType: null,
    targetId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: "",
    ...partial,
  };
}

describe("lastUnansweredUser", () => {
  test("returns the last user when nothing answered yet", () => {
    const user = msg({ id: "u1", role: "user", content: "hi" });
    expect(lastUnansweredUser([user])?.id).toBe("u1");
    expect(lastUnansweredUser([user, msg({ id: "a1", role: "assistant" })])?.id).toBe("u1");
  });

  test("clears once assistant has content or timeline", () => {
    const user = msg({ id: "u1", role: "user", content: "hi" });
    expect(lastUnansweredUser([user, msg({ id: "a1", role: "assistant", content: "ok" })])).toBeNull();
    expect(
      lastUnansweredUser([
        user,
        msg({
          id: "a1",
          role: "assistant",
          timeline: [{
            seq: 1,
            eventType: "thinking",
            callId: null,
            tool: null,
            content: "…",
            input: null,
            output: null,
            createdAt: "",
          }],
        }),
      ]),
    ).toBeNull();
  });

  test("assistantHasOutput treats failure and no_response as output", () => {
    expect(assistantHasOutput(msg({ id: "a", role: "assistant", failureReason: "x" }))).toBe(true);
    expect(assistantHasOutput(msg({ id: "a", role: "assistant", messageKind: "no_response" }))).toBe(true);
    expect(assistantHasOutput(msg({ id: "a", role: "assistant" }))).toBe(false);
  });
});
