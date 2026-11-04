import { describe, expect, it } from "vitest";
import type { RunEvent } from "../domain/types";
import { collectLiveRuns } from "./agent-live-status";

function run(runId: string, sessionId: string, agentId: string, status: RunEvent["status"]): RunEvent {
  return {
    runId, sessionId, agentId, status,
    chunk: null, error: null, timelineItem: null, elapsedMs: null,
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

  it("ignores other sessions, finished runs and pending placeholders", () => {
    const states = {
      a: run("a", "other", "ag1", "running"),
      b: run("b", "s1", "ag1", "completed"),
      c: run("pending:x", "s1", "ag2", "running"),
    };
    expect(collectLiveRuns(states, ["s1"])).toEqual([]);
  });
});
