import { describe, expect, it } from "vitest";
import type { ChatMessage, RunStatus, TimelineItem } from "../../domain/types";
import { composerPlanHintKey, derivePlanStates, latestAwaitingPlan, planKey } from "./plan-state";
import { groupTimeline } from "./timeline-display";

let seq = 0;

function event(eventType: TimelineItem["eventType"], input: Record<string, unknown>): TimelineItem {
  seq += 1;
  return { seq, eventType, callId: null, tool: null, content: null, input, output: null, createdAt: "now" };
}

function plan(planId: string, revision: number, extra: Record<string, unknown> = {}) {
  return event("plan", {
    planId,
    revision,
    title: `计划 ${planId} r${revision}`,
    summary: "摘要",
    markdown: "# 计划\n\n1. 第一步",
    steps: [{ id: "1", text: "第一步", status: "pending" }],
    risks: [],
    questions: [],
    structured: true,
    ...extra,
  });
}

function action(planId: string, revision: number, kind: string) {
  return event("plan_action", { planId, revision, action: kind, by: { type: "user" } });
}

function assistant(id: string, timeline: TimelineItem[], extra: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id, sessionId: "s", role: "assistant", content: "", runId: id, failureReason: null, elapsedMs: null,
    messageKind: "message", quickActions: [], timeline, attachments: [], createdAt: "now", ...extra,
  };
}

function user(id: string, sourceMeta: Record<string, unknown> | null = null): ChatMessage {
  return {
    id, sessionId: "s", role: "user", content: "", runId: null, failureReason: null, elapsedMs: null,
    messageKind: "message", quickActions: [], timeline: [], attachments: [], createdAt: "now", sourceMeta,
  };
}

function approveFlow(executor: ChatMessage[]) {
  return [
    user("u1"),
    assistant("a1", [plan("p", 1), action("p", 1, "approve")]),
    user("u2", { planAction: "approve", planId: "p", revision: 1 }),
    ...executor,
  ];
}

describe("derivePlanStates", () => {
  it("awaits approval when nothing acted on the plan", () => {
    const states = derivePlanStates([user("u1"), assistant("a1", [plan("p", 1)])]);
    expect(states.byKey.get(planKey("p", 1))?.status).toBe("awaiting_approval");
    expect(latestAwaitingPlan(states)?.artifact.title).toBe("计划 p r1");
  });

  it("supersedes the old revision once a newer revision exists", () => {
    const states = derivePlanStates([
      assistant("a1", [plan("p", 1), action("p", 1, "refine")]),
      user("u2", { planAction: "refine", planId: "p", revision: 1 }),
      assistant("a2", [plan("p", 2)]),
    ]);
    expect(states.byKey.get(planKey("p", 1))?.status).toBe("superseded");
    expect(states.byKey.get(planKey("p", 2))?.status).toBe("awaiting_approval");
    expect(states.latestAwaitingKey).toBe(planKey("p", 2));
  });

  it("supersedes an older revision even without a recorded refine action", () => {
    const states = derivePlanStates([assistant("a1", [plan("p", 1)]), assistant("a2", [plan("p", 2)])]);
    expect(states.byKey.get(planKey("p", 1))?.status).toBe("superseded");
  });

  it("follows approve → executing → executed / failed through the execution run", () => {
    const statuses: Record<string, RunStatus> = {};
    const runStatus = (message: ChatMessage) => statuses[message.id];
    const key = planKey("p", 1);
    expect(derivePlanStates(approveFlow([]), runStatus).byKey.get(key)?.status).toBe("approved");
    statuses.a2 = "running";
    expect(derivePlanStates(approveFlow([assistant("a2", [])]), runStatus).byKey.get(key)?.status).toBe("executing");
    statuses.a2 = "completed";
    expect(derivePlanStates(approveFlow([assistant("a2", [])]), runStatus).byKey.get(key)?.status).toBe("executed");
    statuses.a2 = "failed";
    expect(derivePlanStates(approveFlow([assistant("a2", [])]), runStatus).byKey.get(key)?.status).toBe("failed");
    delete statuses.a2;
    const failedFromHistory = approveFlow([assistant("a2", [], { failureReason: "boom" })]);
    expect(derivePlanStates(failedFromHistory).byKey.get(key)?.status).toBe("failed");
  });

  it("marks rejected plans and leaves nothing actionable", () => {
    const states = derivePlanStates([assistant("a1", [plan("p", 1), action("p", 1, "reject")])]);
    expect(states.byKey.get(planKey("p", 1))?.status).toBe("rejected");
    expect(states.latestAwaitingKey).toBeNull();
  });

  it("treats a supersede action (direct message in agent mode) as superseded", () => {
    const states = derivePlanStates([assistant("a1", [plan("p", 1), action("p", 1, "supersede")])]);
    expect(states.byKey.get(planKey("p", 1))?.status).toBe("superseded");
  });

  it("only the latest awaiting plan among several is actionable", () => {
    const states = derivePlanStates([
      assistant("a1", [plan("first", 1)]),
      assistant("a2", [plan("second", 1)]),
    ]);
    expect(states.byKey.get(planKey("first", 1))?.status).toBe("awaiting_approval");
    expect(states.latestAwaitingKey).toBe(planKey("second", 1));
  });

  it("keeps markdown and flags structured=false plans", () => {
    const states = derivePlanStates([assistant("a1", [plan("p", 1, { structured: false, steps: undefined, changedFiles: ["a.ts"] })])]);
    const artifact = states.byKey.get(planKey("p", 1))?.artifact;
    expect(artifact?.structured).toBe(false);
    expect(artifact?.steps).toEqual([]);
    expect(artifact?.changedFiles).toEqual(["a.ts"]);
    expect(artifact?.markdown).toContain("第一步");
  });

  it("picks the composer hint from the pending plan, mode and running round", () => {
    const awaiting = [user("u1"), assistant("a1", [plan("p", 1)])];
    const states = derivePlanStates(awaiting);
    const idle = () => undefined;
    expect(composerPlanHintKey(awaiting, states, "plan", idle)).toBe("chat.plan.hint.refine");
    expect(composerPlanHintKey(awaiting, states, "agent", idle)).toBe("chat.plan.hint.supersede");
    expect(composerPlanHintKey(awaiting, states, "auto", idle)).toBe("chat.plan.hint.supersede");
    const planning = [{ ...user("u2"), requestedMode: "plan" as const }, assistant("a2", [])];
    const running = (message: ChatMessage): RunStatus | undefined => (message.id === "a2" ? "running" : undefined);
    expect(composerPlanHintKey(planning, derivePlanStates(planning), "agent", running)).toBe("chat.plan.hint.steer");
    expect(composerPlanHintKey(planning, derivePlanStates(planning), "agent", idle)).toBe("");
  });

  it("turns mode_change into a mode block and hides bookkeeping events", () => {
    const blocks = groupTimeline([
      event("mode_change", { from: "auto", to: "plan", source: "model" }),
      event("agent_start", { callId: "c1" }),
      event("plan_action", { planId: "p", revision: 1, action: "approve" }),
    ]);
    expect(blocks.map((block) => block.type)).toEqual(["mode"]);
  });

  it("never renders legacy todo-shaped plan events as plans", () => {
    const legacy = event("plan", { entries: [{ content: "跑测试", status: "in_progress" }] });
    const todo = event("todo", { todos: [{ id: "1", text: "写代码", status: "completed" }] });
    const states = derivePlanStates([assistant("a1", [legacy, todo])]);
    expect(states.byKey.size).toBe(0);
    const blocks = groupTimeline([legacy, todo, plan("p", 1)]);
    expect(blocks).toEqual([{ type: "plan", todos: [{ id: "1", text: "写代码", status: "completed" }] }]);
  });
});
