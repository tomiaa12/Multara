import { describe, expect, it } from "vitest";
import { fixtureRunDetail } from "../../../dev/workflow-fixtures";
import { buildRunOverlay, edgeState } from "./run-overlay";
import type { NodeRun } from "../../../domain/workflow";

const nr = (status: string, nextNodeId?: string) => ({ status, nextNodeId }) as unknown as NodeRun;
const gone = { status: "skipped" } as unknown as NodeRun;
const ran = { status: "succeeded" } as unknown as NodeRun;

describe("edgeState", () => {
  it("stays pending while the source is still running", () => {
    expect(edgeState([], "end", undefined)).toBe("pending");
    expect(edgeState([nr("running")], "end", undefined)).toBe("pending");
  });
  it("is dead when the source failed without choosing", () => {
    expect(edgeState([nr("failed")], "end", gone)).toBe("dead");
    expect(edgeState([nr("skipped")], "end", gone)).toBe("dead");
  });
  it("keeps only the next node the source recorded", () => {
    const source = [nr("succeeded", "end")];
    expect(edgeState(source, "end", ran)).toBe("live");
    expect(edgeState(source, "gate", gone)).toBe("dead");
  });
  it("keeps a loop that an earlier round actually took", () => {
    const source = [nr("succeeded", "code"), nr("succeeded", "review")];
    source[1] = { ...source[1], iteration: 1 } as NodeRun;
    expect(edgeState(source, "code", ran)).toBe("live");
    expect(edgeState(source, "review", ran)).toBe("live");
    expect(edgeState(source, "ship", gone)).toBe("dead");
  });
});

describe("buildRunOverlay", () => {
  it("builds node and edge overlays from the fixture run", () => {
    const overlay = buildRunOverlay(fixtureRunDetail("run-1"));
    expect(overlay.nodes.code).toMatchObject({ iteration: 1, multiple: true });
    expect(overlay.nodes.review.status).toBe("waiting_human");
    expect(overlay.edges.e5.state).toBe("live");
    expect(overlay.edges.e5.flowing).toBe(true);
    expect(overlay.edges.e6.state).toBe("live");
    expect(overlay.edges.e6.flowing).toBe(false);
    expect(overlay.edges.e6.count).toBe(1);
    expect(overlay.edges.e2.label).toContain("示例需求");
  });
});
