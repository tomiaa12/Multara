import { describe, expect, it } from "vitest";
import compat from "../../shared/workflow-port-compat.json";
import {
  PORT_TYPES, asGateAction, createNode, effectiveOutputs, isTypeCompatible, isValidNodeId, isValidPortName, uniqueNodeId,
  type NodeDef,
} from "./workflow";
import { fixtureDefinition } from "../dev/workflow-fixtures";

describe("port compatibility", () => {
  it("matches shared/workflow-port-compat.json for every pair", () => {
    const table = compat as Record<string, string[]>;
    for (const source of PORT_TYPES) {
      for (const target of PORT_TYPES) {
        expect(isTypeCompatible(source, target), `${source} -> ${target}`).toBe(table[source].includes(target));
      }
    }
  });

  it("number -> json ok, json -> number not", () => {
    expect(isTypeCompatible("number", "json")).toBe(true);
    expect(isTypeCompatible("json", "number")).toBe(false);
    expect(isTypeCompatible("any", "file")).toBe(true);
  });
});

describe("implicit ports (mirror of model.rs)", () => {
  const def = fixtureDefinition();
  const names = (node: NodeDef) => effectiveOutputs(node, def).map((port) => port.name);

  it("script exposes exit_code / stdout / $result", () => {
    const script = createNode("script", def, { x: 0, y: 0 });
    expect(names(script)).toEqual(expect.arrayContaining(["exit_code", "stdout", "stderr", "log", "ok", "parsed", "$result", "$out", "$error"]));
  });

  it("switch exposes case ports, default and exhausted", () => {
    const sw = def.nodes.find((node) => node.id === "check") as NodeDef;
    expect(names(sw)).toEqual(expect.arrayContaining(["pass", "fail", "exhausted"]));
  });

  it("a new agent waits for confirmation only when that option is on", () => {
    const agent = createNode("agent", def, { x: 0, y: 0 });
    expect(agent.config).toMatchObject({ humanConfirm: false, route: "auto" });
  });
});

describe("ids", () => {
  it("generates unique ids matching the backend pattern", () => {
    expect(uniqueNodeId("agent", ["agent", "agent_1"])).toBe("agent_2");
    expect(uniqueNodeId("agent", [])).toBe("agent");
    expect(isValidNodeId(uniqueNodeId("2x", []))).toBe(true);
    expect(isValidNodeId("Bad-id")).toBe(false);
  });

  it("rejects empty or missing port names without throwing", () => {
    expect(isValidPortName("spec")).toBe(true);
    expect(isValidPortName("需求说明")).toBe(true);
    expect(isValidPortName("  端口 ")).toBe(true);
    expect(isValidPortName("   ")).toBe(false);
    expect(isValidPortName("")).toBe(false);
    expect(isValidPortName(undefined)).toBe(false);
    expect(isValidPortName(null)).toBe(false);
  });

  it("asGateAction only accepts workflow.gate actions", () => {
    expect(asGateAction({ type: "workflow.gate", nodeRunId: "n", decision: "approve" })).not.toBeNull();
    expect(asGateAction({ type: "other" })).toBeNull();
    expect(asGateAction(null)).toBeNull();
  });
});
