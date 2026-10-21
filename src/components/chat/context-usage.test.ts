import { describe, expect, test } from "vitest";
import type { ChatMessage } from "../../domain/types";
import { estimateContextUsage, formatTokenCount } from "./context-usage";

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

  test("formats thousands", () => {
    expect(formatTokenCount(904)).toBe("904");
    expect(formatTokenCount(1700)).toBe("1.7K");
    expect(formatTokenCount(166700)).toBe("166.7K");
  });
});
