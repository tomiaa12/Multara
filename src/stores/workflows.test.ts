import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { fixtureDetail } from "../dev/workflow-fixtures";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("../services/backend", () => ({
  getWorkflow: vi.fn(async (id: string) => fixtureDetail(id)),
  saveWorkflowDraft: vi.fn(async () => ({ updatedAt: "2026-11-18T09:00:00Z", diagnostics: [] })),
  listWorkflows: vi.fn(async () => []),
  getWorkflowRun: vi.fn(),
}));

import * as backend from "../services/backend";
import { useWorkflowsStore } from "./workflows";

describe("workflows store", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    setActivePinia(createPinia());
    vi.mocked(backend.saveWorkflowDraft).mockClear();
  });

  async function loaded() {
    const store = useWorkflowsStore();
    await store.loadWorkflow("wf-req");
    return store;
  }

  it("undo / redo restores the draft", async () => {
    const store = await loaded();
    const count = store.draft!.nodes.length;
    const id = store.addNode("script", { x: 0, y: 0 });
    expect(store.draft!.nodes).toHaveLength(count + 1);
    expect(store.canUndo).toBe(true);
    store.undo();
    expect(store.draft!.nodes).toHaveLength(count);
    expect(store.canRedo).toBe(true);
    store.redo();
    expect(store.draft!.nodes.some((node) => node.id === id)).toBe(true);
    // 新的修改会清空重做栈
    store.undo();
    store.addNode("http", { x: 0, y: 0 });
    expect(store.canRedo).toBe(false);
  });

  it("coalesces rapid edits with the same key into one undo step", async () => {
    const store = await loaded();
    store.updateNode("code", (node) => { node.title = "a"; });
    store.updateNode("code", (node) => { node.title = "ab"; });
    store.updateNode("code", (node) => { node.title = "abc"; });
    store.undo();
    expect(store.draft!.nodes.find((node) => node.id === "code")!.title).toBe("编写代码");
  });

  it("debounces autosave (800ms) with the optimistic-lock timestamp", async () => {
    const store = await loaded();
    store.addNode("script", { x: 0, y: 0 });
    store.addNode("http", { x: 10, y: 0 });
    expect(backend.saveWorkflowDraft).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(799);
    expect(backend.saveWorkflowDraft).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(2);
    expect(backend.saveWorkflowDraft).toHaveBeenCalledTimes(1);
    expect(vi.mocked(backend.saveWorkflowDraft).mock.calls[0][2]).toBe(store.workflow!.draftUpdatedAt);
    expect(store.updatedAt).toBe("2026-11-18T09:00:00Z");
    expect(store.saveState).toBe("saved");
  });

  it("paste regenerates node and edge ids and fixes internal edges", async () => {
    const store = await loaded();
    const payload = store.copyNodes(["code", "test", "check"])!;
    expect(payload.nodes).toHaveLength(3);
    // code→test(e3)、test→check(e4) 在选区内；check→code 回边也在选区内
    expect(payload.edges.map((edge) => edge.id).sort()).toEqual(["e3", "e4", "e6"]);
    const before = store.draft!.nodes.length;
    const ids = store.pasteNodes(payload);
    expect(ids).toEqual(["code_2", "test_2", "check_2"]);
    expect(store.draft!.nodes).toHaveLength(before + 3);
    const pasted = store.draft!.edges.filter((edge) => edge.source.endsWith("_2"));
    expect(pasted).toHaveLength(3);
    for (const edge of pasted) expect(edge.target.endsWith("_2")).toBe(true);
    const all = store.draft!.edges.map((edge) => edge.id);
    expect(new Set(all).size).toBe(all.length);
    // 回边属性保留
    expect(pasted.find((edge) => edge.maxIterations)?.maxIterations).toBe(3);
  });

  it("never deletes the trigger and removes dangling edges", async () => {
    const store = await loaded();
    store.removeNodes(["start", "test"]);
    expect(store.draft!.nodes.some((node) => node.id === "start")).toBe(true);
    expect(store.draft!.nodes.some((node) => node.id === "test")).toBe(false);
    expect(store.draft!.edges.some((edge) => edge.source === "test" || edge.target === "test")).toBe(false);
  });
});
