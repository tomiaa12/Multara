// @vitest-environment node
import { describe, expect, test } from "vitest";
import type { RunEvent } from "../../domain/types";
import { clipText, diagnosticFileName, sessionRunStates, slimRunState } from "./export-session-diagnostics";

function event(patch: Partial<RunEvent> = {}): RunEvent {
  return {
    runId: "run-1",
    sessionId: "session-1",
    agentId: "agent-1",
    status: "running",
    chunk: "x".repeat(20),
    error: null,
    timelineItem: null,
    elapsedMs: 12,
    ...patch,
  };
}

describe("chat diagnostic export", () => {
  test("clips long client text and drops file snapshots", () => {
    const slim = slimRunState(event({
      error: "e".repeat(4_004),
      snapshotContent: "secret file",
      timelineItem: {
        seq: 4,
        eventType: "thinking",
        callId: null,
        tool: null,
        content: "thinking",
        input: { huge: true },
        output: "tool output",
        createdAt: "2026-01-01T00:00:00.000Z",
      },
    }));
    expect(slim.error).toBe(`${"e".repeat(4_000)}\n…[clipped 4 chars]`);
    expect(slim).not.toHaveProperty("snapshotContent");
    expect(slim.timelineItem).toEqual({
      seq: 4,
      eventType: "thinking",
      tool: null,
      status: null,
      createdAt: "2026-01-01T00:00:00.000Z",
      content: "thinking",
    });
    expect(clipText("ok", 8)).toBe("ok");
  });

  test("keeps only the live state for the exported session", () => {
    const states = sessionRunStates({
      a: event(),
      b: event({ runId: "run-2", sessionId: "other" }),
    }, "session-1");
    expect(states.map((item) => item.runId)).toEqual(["run-1"]);
  });

  test("builds a readable file name from the chat title", () => {
    expect(diagnosticFileName("Fix login", "11111111-2222")).toBe("multara-chat-Fix-login.json");
    expect(diagnosticFileName("   ", "abcdef12-more")).toBe("multara-chat-abcdef12.json");
  });
});