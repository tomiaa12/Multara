import { describe, expect, it } from "vitest";
import { ancestors, checkConnection, descendants, forwardEdges, isBackEdge, loopBody, remapClipboard, topoOrder } from "./workflow-graph";
import { fixtureDefinition } from "../dev/workflow-fixtures";
import type { NodeDef } from "./workflow";

const def = fixtureDefinition();
const byId = (id: string) => def.nodes.find((node) => node.id === id) as NodeDef;

describe("workflow-graph", () => {
  it("识别回边（带 maxIterations）并从前向图中排除", () => {
    expect(def.edges.filter(isBackEdge).map((edge) => edge.id)).toEqual(["e6"]);
    expect(forwardEdges(def).some((edge) => edge.id === "e6")).toBe(false);
  });

  it("祖先 / 后代只看前向边", () => {
    expect([...ancestors(def, "check")].sort()).toEqual(["code", "prd", "start", "test"]);
    expect(ancestors(def, "start").size).toBe(0);
    expect([...descendants(def, "code")].sort()).toEqual(["check", "done", "review", "ship", "test"]);
    // 回边不让 check 成为 code 的祖先
    expect(ancestors(def, "code").has("check")).toBe(false);
  });

  it("循环体 = 回边目标 ∪ 其前向后代", () => {
    expect([...loopBody(def, "code")].sort()).toEqual(["check", "code", "done", "review", "ship", "test"]);
  });

  it("连到祖先 ⇒ 只能从 switch / gate 建回边", () => {
    expect(checkConnection(def, byId("check"), byId("code")).kind).toBe("back");
    expect(checkConnection(def, byId("review"), byId("code")).kind).toBe("back");
    expect(checkConnection(def, byId("test"), byId("code"))).toEqual({ kind: "invalid", reason: "back_source" });
    expect(checkConnection(def, byId("prd"), byId("test")).kind).toBe("forward");
    expect(checkConnection(def, byId("code"), byId("code"))).toEqual({ kind: "invalid", reason: "self" });
    expect(checkConnection(def, byId("code"), byId("start")).kind).toBe("invalid");
  });

  it("拓扑序按前向边排列", () => {
    const order = topoOrder(def);
    expect(order.indexOf("start")).toBeLessThan(order.indexOf("prd"));
    expect(order.indexOf("code")).toBeLessThan(order.indexOf("check"));
  });

  it("复制粘贴重新生成 id 并修正内部连线，丢弃选区外连线", () => {
    const nodes = [byId("code"), byId("test")];
    const edges = def.edges.filter((edge) => ["e3"].includes(edge.id) || edge.id === "e2");
    const result = remapClipboard(nodes, edges, def.nodes.map((node) => node.id), def.edges.map((edge) => edge.id), { x: 40, y: 40 });
    expect(result.nodes.map((node) => node.id)).toEqual(["code_2", "test_2"]);
    expect(result.edges).toHaveLength(1);
    expect(result.edges[0]).toMatchObject({ source: "code_2", target: "test_2" });
    expect(def.edges.map((edge) => edge.id)).not.toContain(result.edges[0].id);
    expect(result.nodes[0].position.x).toBe(byId("code").position.x + 40);
  });

  it("缺少 position 的内置模板节点也能投放", () => {
    const bare = { ...byId("code"), position: undefined as unknown as { x: number; y: number } };
    const result = remapClipboard([bare], [], def.nodes.map((node) => node.id), [], { x: 120, y: 80 });
    expect(result.nodes).toHaveLength(1);
    expect(result.nodes[0].position).toEqual({ x: 120, y: 80 });
  });
});
