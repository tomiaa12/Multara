import { describe, expect, it } from "vitest";
import type { RunEvent } from "../domain/types";
import {
  collectAllLiveRuns,
  collectLiveRuns,
  pickSessionLiveRun,
  resolveRunLiveWorkStatus,
  resolveSessionLiveWorkStatus,
  retryItemFromRun,
  type LiveCapacity,
} from "./agent-live-status";

function run(
  runId: string,
  sessionId: string,
  agentId: string,
  status: RunEvent["status"],
  extra: Partial<RunEvent> = {},
): RunEvent {
  return {
    runId, sessionId, agentId, status,
    chunk: null, error: null, timelineItem: null, elapsedMs: null,
    ...extra,
  };
}

describe("collectLiveRuns", () => {
  it("keeps one run per agent, preferring running over queued", () => {
    const states = {
      a: run("a", "s1", "ag1", "queued"),
      b: run("b", "s2", "ag1", "running"),
      c: run("c", "s1", "ag2", "queued"),
    };
    const live = collectLiveRuns(states, ["s1", "s2"]);
    expect(live.map((item) => [item.agentId, item.status])).toEqual([["ag1", "running"], ["ag2", "queued"]]);
  });

  it("counts a channel session that is not on the current page", () => {
    const states = {
      a: run("a", "channel-session", "ag1", "running"),
      b: run("b", "local", "ag2", "completed"),
    };
    expect(collectAllLiveRuns(states).map((item) => item.agentId)).toEqual(["ag1"]);
  });

  it("ignores other sessions, finished runs and pending placeholders", () => {
    const states = {
      a: run("a", "other", "ag1", "running"),
      b: run("b", "s1", "ag1", "completed"),
      c: run("pending:x", "s1", "ag2", "running"),
    };
    expect(collectLiveRuns(states, ["s1"])).toEqual([]);
  });
});

const cursorCapacity: LiveCapacity = {
  agents: [
    { id: "ag1", runtimeId: "cursor", maxConcurrentTasks: 1 },
    { id: "ag2", runtimeId: "cursor", maxConcurrentTasks: 3 },
  ],
  runtimes: [{ id: "cursor", maxConcurrent: 3 }],
};

describe("resolveSessionLiveWorkStatus", () => {
  it("maps lone queued to sending (启动中) while the runtime still has a slot", () => {
    const states = { a: run("a", "s1", "ag1", "queued") };
    expect(resolveSessionLiveWorkStatus({
      runStates: states,
      sessionId: "s1",
      capacity: cursorCapacity,
    })).toBe("sending");
  });

  it("does not queue on the agent itself while the bound runtime still has a slot", () => {
    const states = {
      a: run("a", "other", "ag1", "running"),
      b: run("b", "s1", "ag1", "queued"),
    };
    expect(resolveSessionLiveWorkStatus({
      runStates: states,
      sessionId: "s1",
      capacity: cursorCapacity,
    })).toBe("sending");
  });

  it("shows queued when the bound runtime is globally full, even for another agent", () => {
    const states = {
      a: run("a", "s-a", "ag2", "running"),
      b: run("b", "s-b", "ag2", "running"),
      c: run("c", "s-c", "ag2", "running"),
      d: run("d", "s1", "ag1", "queued"),
    };
    expect(resolveSessionLiveWorkStatus({
      runStates: states,
      sessionId: "s1",
      capacity: cursorCapacity,
    })).toBe("queued");
  });

  it("prefers running when present", () => {
    const states = {
      a: run("a", "s1", "ag1", "running"),
      b: run("b", "s1", "ag2", "queued"),
    };
    expect(resolveSessionLiveWorkStatus({
      runStates: states,
      sessionId: "s1",
      capacity: cursorCapacity,
    })).toBe("running");
  });

  it("falls back to sending while processing with no live runs", () => {
    expect(resolveSessionLiveWorkStatus({
      runStates: {},
      sessionId: "s1",
      processing: true,
    })).toBe("sending");
  });
});

describe("pickSessionLiveRun", () => {
  it("prefers running over a more recently active queued run", () => {
    const states = {
      a: run("a", "s1", "ag1", "queued", { lastEventAtMs: 900 }),
      b: run("b", "s1", "ag2", "running", { lastEventAtMs: 100 }),
    };
    expect(pickSessionLiveRun(states, "s1")?.runId).toBe("b");
  });

  it("picks the most recently active running run", () => {
    const states = {
      a: run("a", "s1", "ag1", "running", { lastEventAtMs: 100 }),
      b: run("b", "s1", "ag2", "running", { lastEventAtMs: 900 }),
    };
    expect(pickSessionLiveRun(states, "s1")?.runId).toBe("b");
  });

  it("ignores other sessions, finished runs and pending placeholders", () => {
    const states = {
      a: run("a", "other", "ag1", "running"),
      b: run("b", "s1", "ag1", "completed"),
      c: run("pending:x", "s1", "ag2", "running"),
    };
    expect(pickSessionLiveRun(states, "s1")).toBeNull();
  });
});

describe("retryItemFromRun", () => {
  it("returns the latest broadcast timeline item for the run", () => {
    const item = {
      seq: 3,
      eventType: "error" as const,
      callId: null,
      tool: null,
      content: "Request timed out, retrying (1/10)",
      input: { kind: "retry", attempt: 1, maxRetries: 10 },
      output: null,
      createdAt: "2026-01-01T00:00:00.000Z",
    };
    const states = { a: run("a", "s1", "ag1", "running", { timelineItem: item }) };
    expect(retryItemFromRun(states, "a")).toBe(item);
    expect(retryItemFromRun(states, "missing")).toBeNull();
  });
});

describe("resolveRunLiveWorkStatus", () => {
  it("shows starting while the runtime has a free slot, queued once that slot is full", () => {
    const solo = { a: run("a", "s1", "ag2", "queued") };
    expect(resolveRunLiveWorkStatus(
      { runId: "a", agentId: "ag2", status: "queued" },
      solo,
      cursorCapacity,
    )).toBe("sending");

    const full = {
      a: run("a", "s-a", "ag2", "running"),
      b: run("b", "s-b", "ag2", "running"),
      c: run("c", "s-c", "ag2", "running"),
      d: run("d", "s1", "ag1", "queued"),
    };
    expect(resolveRunLiveWorkStatus(
      { runId: "d", agentId: "ag1", status: "queued" },
      full,
      cursorCapacity,
    )).toBe("queued");
  });
});
