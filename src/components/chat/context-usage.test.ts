// @vitest-environment node
import { describe, expect, test } from "vitest";
import type { ChatMessage } from "../../domain/types";
import {
  estimateContextUsage,
  formatTokenCount,
  listContextAgentIds,
  messagesForContextAgent,
} from "./context-usage";

function message(partial: Partial<ChatMessage> & Pick<ChatMessage, "content">): ChatMessage {
  return {
    id: "m",
    sessionId: "s",
    role: "user",
    runId: null,
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

describe("estimateContextUsage", () => {
  test("counts conversation text and attachment bytes", () => {
    const usage = estimateContextUsage([
      message({
        content: "a".repeat(8),
        attachments: [{ id: "a", name: "note", path: "note", mimeType: "text/plain", size: 8 }],
      }),
    ]);
    expect(usage.slices.find((slice) => slice.id === "conversation")?.tokens).toBe(3);
    expect(usage.slices.find((slice) => slice.id === "attachments")?.tokens).toBe(2);
    expect(usage.used).toBe(5);
    expect(usage.percent).toBeCloseTo((5 / 200_000) * 100);
  });

  test("prefers reported usage slices when present", () => {
    const usage = estimateContextUsage([
      message({
        role: "assistant",
        agentId: "miku",
        content: "ignored when usage exists",
        usage: {
          inputTokens: 100,
          outputTokens: 40,
          cacheReadTokens: 10,
          cacheWriteTokens: 0,
        },
      }),
    ]);
    expect(usage.slices.map((slice) => slice.id)).toEqual(["input", "output", "cacheRead"]);
    expect(usage.used).toBe(150);
  });

  test("filters consumption by agent", () => {
    const messages = [
      message({ id: "u1", role: "user", content: "a".repeat(40), targetType: "agent", targetId: "miku" }),
      message({ id: "a1", role: "assistant", agentId: "miku", content: "b".repeat(40) }),
      message({ id: "a2", role: "assistant", agentId: "coder", content: "c".repeat(80) }),
    ];
    expect(listContextAgentIds(messages)).toEqual(["miku", "coder"]);
    expect(messagesForContextAgent(messages, "miku").map((item) => item.id)).toEqual(["u1", "a1"]);
    const miku = estimateContextUsage(messages, "miku");
    const all = estimateContextUsage(messages);
    expect(miku.used).toBeLessThan(all.used);
    expect(miku.slices.find((slice) => slice.id === "conversation")?.tokens).toBe(20);
  });

  test("formats thousands", () => {
    expect(formatTokenCount(904)).toBe("904");
    expect(formatTokenCount(1700)).toBe("1.7K");
    expect(formatTokenCount(166700)).toBe("166.7K");
  });
});
