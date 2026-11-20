import { describe, expect, it } from "vitest";
import { fixtureRunDetail } from "../../../dev/workflow-fixtures";
import { buildRunOverlay, edgeState } from "./run-overlay";
import type { NodeRun } from "../../../domain/workflow";

const nr = (status: string, firedPorts: string[] = []) => ({ status, firedPorts }) as unknown as NodeRun;

describe("edgeState", () => {
  it("is pending without a decided source", () => {
    expect(edgeState(undefined, "agent", "text", "fail_run", [])).toBe("pending");
    expect(edgeState(nr("running"), "agent", "text", "fail_run", [])).toBe("pending");
  });
  it("is dead for skipped sources", () => {
    expect(edgeState(nr("skipped"), "agent", "text", "fail_run", [])).toBe("dead");
  });
  it("branching sources only fire selected ports", () => {
    const ports = ["pass", "fail", "exhausted"];
    expect(edgeState(nr("succeeded", ["pass"]), "switch", "pass", "fail_run", ports)).toBe("live");
    expect(edgeState(nr("succeeded", ["pass"]), "switch", "fail", "fail_run", ports)).toBe("dead");
    expect(edgeState(nr("succeeded", ["pass"]), "switch", "$out", "fail_run", ports)).toBe("live");
  });
  it("failed with onError=continue only fires $error", () => {
    expect(edgeState(nr("failed"), "script", "$error", "continue", [])).toBe("live");
    expect(edgeState(nr("failed"), "script", "stdout", "continue", [])).toBe("dead");
    expect(edgeState(nr("failed"), "script", "$error", "fail_run", [])).toBe("dead");
  });
  it("$error is dead on success", () => {
    expect(edgeState(nr("succeeded"), "script", "$error", "continue", [])).toBe("dead");
  });
});

describe("buildRunOverlay", () => {
  it("builds node and edge overlays from the fixture run", () => {
    const overlay = buildRunOverlay(fixtureRunDetail("run-1"));
    expect(overlay.nodes.code).toMatchObject({ iteration: 1, multiple: true });
    expect(overlay.nodes.review.status).toBe("waiting_human");
    expect(overlay.edges.e5.state).toBe("live");
    expect(overlay.edges.e6.state).toBe("dead");
    expect(overlay.edges.e6.count).toBe(1);
    expect(overlay.edges.e2.label).toContain("示例需求");
  });
});
