import { describe, expect, it } from "vitest";
import type { TimelineItem } from "../../domain/types";
import {
  buildLanes,
  buildRunLogSteps,
  stepHaystack,
  timelineFromMessages,
} from "./build-run-log";

function item(partial: Partial<TimelineItem> & Pick<TimelineItem, "seq" | "eventType">): TimelineItem {
  return {
    callId: null,
    tool: null,
    content: null,
    input: null,
    output: null,
    createdAt: "2026-01-01T00:00:00.000Z",
    ...partial,
  };
}

describe("buildRunLogSteps", () => {
  it("pairs tool_use and tool_result by callId", () => {
    const steps = buildRunLogSteps([
      item({
        seq: 1,
        eventType: "tool_use",
        callId: "c1",
        tool: "Bash",
        input: { command: "ls" },
        createdAt: "2026-01-01T00:00:00.000Z",
      }),
      item({
        seq: 2,
        eventType: "tool_result",
        callId: "c1",
        tool: "Bash",
        output: "ok",
        createdAt: "2026-01-01T00:00:02.000Z",
      }),
    ]);
    expect(steps).toHaveLength(1);
    expect(steps[0]).toMatchObject({ kind: "call", tool: "Bash", durationMs: 2000 });
  });

  it("builds model/tool lanes from call spans", () => {
    const steps = buildRunLogSteps([
      item({
        seq: 1,
        eventType: "thinking",
        content: "plan",
        createdAt: "2026-01-01T00:00:00.000Z",
      }),
      item({
        seq: 2,
        eventType: "tool_use",
        callId: "c1",
        tool: "Read",
        input: { path: "a.ts" },
        createdAt: "2026-01-01T00:00:01.000Z",
      }),
      item({
        seq: 3,
        eventType: "tool_result",
        callId: "c1",
        tool: "Read",
        output: "code",
        createdAt: "2026-01-01T00:00:03.000Z",
      }),
      item({
        seq: 4,
        eventType: "text",
        content: "done",
        createdAt: "2026-01-01T00:00:04.000Z",
      }),
    ]);
    const lanes = buildLanes(steps, "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:04.000Z");
    expect(lanes).not.toBeNull();
    expect(lanes!.toolMs).toBe(2000);
    expect(lanes!.modelMs).toBe(2000);
    expect(lanes!.totalMs).toBe(4000);
  });

  it("filters search haystack", () => {
    const steps = buildRunLogSteps([
      item({
        seq: 1,
        eventType: "tool_use",
        callId: "c1",
        tool: "Bash",
        input: { command: "pnpm test" },
      }),
    ]);
    expect(stepHaystack(steps[0]!)).toContain("pnpm test");
  });
});

describe("timelineFromMessages", () => {
  it("collects assistant timeline for one run", () => {
    const timeline = timelineFromMessages(
      [
        {
          id: "m1",
          sessionId: "s1",
          role: "assistant",
          content: "",
          runId: "r1",
          failureReason: null,
          elapsedMs: 1200,
          messageKind: "message",
          quickActions: [],
          timeline: [
            item({ seq: 2, eventType: "text", content: "b", createdAt: "2026-01-01T00:00:02.000Z" }),
            item({ seq: 1, eventType: "thinking", content: "a", createdAt: "2026-01-01T00:00:01.000Z" }),
          ],
          attachments: [],
          createdAt: "2026-01-01T00:00:00.000Z",
        },
        {
          id: "m2",
          sessionId: "s1",
          role: "assistant",
          content: "",
          runId: "r2",
          failureReason: null,
          elapsedMs: null,
          messageKind: "message",
          quickActions: [],
          timeline: [item({ seq: 1, eventType: "text", content: "other" })],
          attachments: [],
          createdAt: "2026-01-01T00:00:00.000Z",
        },
      ],
      "r1",
    );
    expect(timeline.map((item) => item.seq)).toEqual([1, 2]);
  });
});
