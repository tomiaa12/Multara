import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import type { CanvasInstance } from "../domain/types";

const backend = vi.hoisted(() => ({
  listCanvasInstances: vi.fn(),
  ensureCanvasHosted: vi.fn(),
  canvasHostedUrl: vi.fn(),
  setCanvasVisible: vi.fn(),
  setCanvasPinned: vi.fn(),
  openRegisteredCanvas: vi.fn(),
  closeCanvas: vi.fn(),
  resolveUiOutbound: vi.fn(),
}));
vi.mock("../services/backend", () => backend);
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { BACKGROUND_TTL_MS, MAX_BACKGROUND_FRAMES, useCanvasStore } from "./canvas";
import { fillComposerFromUi, formatUiContexts, registerComposerFiller } from "../components/chat/ui-outbound";

function instance(sessionId: string, instanceId: string, partial: Partial<CanvasInstance> = {}): CanvasInstance {
  return {
    instanceId,
    sessionId,
    canvasId: instanceId,
    title: `Board ${instanceId}`,
    status: "",
    input: null,
    hasContext: false,
    createdByAgent: "miku",
    visible: true,
    createdAt: "2026-11-07T00:00:00Z",
    updatedAt: "2026-11-07T00:00:00Z",
    ...partial,
  };
}

describe("canvas store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    for (const fn of Object.values(backend)) fn.mockReset();
  });

  it("groups instances by session and drops urls of closed ones", async () => {
    const store = useCanvasStore();
    backend.listCanvasInstances.mockResolvedValueOnce({
      enabled: true,
      instances: [instance("s1", "a"), instance("s1", "b")],
      available: [{ canvasId: "c", title: "C", description: "" }],
    });
    await store.refresh("s1");
    backend.listCanvasInstances.mockResolvedValueOnce({ enabled: true, instances: [instance("s2", "x")], available: [] });
    await store.refresh("s2");
    expect(store.listFor("s1").map((item) => item.instanceId)).toEqual(["a", "b"]);
    expect(store.listFor("s2").map((item) => item.instanceId)).toEqual(["x"]);
    expect(store.available.s1.map((item) => item.canvasId)).toEqual(["c"]);

    backend.ensureCanvasHosted.mockResolvedValue("http://127.0.0.1:1/t/");
    await store.ensureUrl("s1", "b");
    store.claim("s1", "b", "view-1");
    backend.listCanvasInstances.mockResolvedValueOnce({ enabled: true, instances: [instance("s1", "a")], available: [] });
    await store.refresh("s1");
    expect(store.urls["s1/b"]).toBeUndefined();
    expect(store.claimedBy("s1", "b")).toBeNull();
  });

  it("closing a tab only changes visibility; closing the canvas ends it", async () => {
    const store = useCanvasStore();
    backend.listCanvasInstances.mockResolvedValue({ enabled: true, instances: [instance("s1", "a")], available: [] });
    await store.refresh("s1");
    await store.setVisible("s1", "a", false);
    expect(backend.setCanvasVisible).toHaveBeenCalledWith("s1", "a", false);
    expect(backend.closeCanvas).not.toHaveBeenCalled();
    expect(store.listFor("s1")[0].visible).toBe(false);
    await store.close("s1", "a");
    expect(backend.closeCanvas).toHaveBeenCalledWith("s1", "a");
    expect(store.listFor("s1")).toEqual([]);
  });

  it("show marks visible and raises a show request", async () => {
    const store = useCanvasStore();
    await store.show("s1", "a");
    expect(backend.setCanvasVisible).toHaveBeenCalledWith("s1", "a", true);
    expect(store.showRequest).toMatchObject({ sessionId: "s1", instanceId: "a" });
  });

  it("one live view per instance", () => {
    const store = useCanvasStore();
    store.claim("s1", "a", "workbench");
    expect(store.claimedBy("s1", "a")).toBe("workbench");
    // 别的视图释放不了不属于它的认领
    store.release("s1", "a", "modal");
    expect(store.claimedBy("s1", "a")).toBe("workbench");
    store.claim("s1", "a", "modal");
    expect(store.claimedBy("s1", "a")).toBe("modal");
    store.release("s1", "a", "modal");
    expect(store.claimedBy("s1", "a")).toBeNull();
  });

  it("page context replaces the previous copy from the same source", () => {
    const store = useCanvasStore();
    store.attachContext({ sessionId: "s1", source: "canvas", sourceId: "a", label: "A", text: "one", data: null });
    store.attachContext({ sessionId: "s1", source: "canvas", sourceId: "a", label: "A", text: "two", data: { n: 2 } });
    store.attachContext({ sessionId: "s1", source: "canvas", sourceId: "b", label: "B", text: "b", data: null });
    store.attachContext({ sessionId: "s2", source: "canvas", sourceId: "a", label: "A", text: "other", data: null });
    expect(store.contextsFor("s1").map((item) => item.text)).toEqual(["two", "b"]);
    const taken = store.takeContexts("s1");
    expect(taken).toHaveLength(2);
    expect(store.contextsFor("s1")).toEqual([]);
    expect(store.contextsFor("s2")).toHaveLength(1);
    const text = formatUiContexts(taken);
    expect(text).toContain("two");
    expect(text).toContain('"n": 2');
  });

  it("pinned instances from other chats can be shown and closed with their own session", async () => {
    const store = useCanvasStore();
    backend.listCanvasInstances.mockResolvedValueOnce({
      enabled: true,
      instances: [instance("s1", "mine"), instance("s0", "board", { pinned: true })],
      available: [],
    });
    await store.refresh("s1");
    expect(store.shownIn("s1", "s0", "board")).toBe(true);
    expect(store.shownIn("s2", "s0", "board")).toBe(false);
    await store.setPinned("s0", "board", false);
    expect(backend.setCanvasPinned).toHaveBeenCalledWith("s0", "board", false);
    expect(store.listFor("s1").find((item) => item.instanceId === "board")?.pinned).toBe(false);
    await store.close("s0", "board");
    expect(backend.closeCanvas).toHaveBeenCalledWith("s0", "board");
    expect(store.listFor("s1").map((item) => item.instanceId)).toEqual(["mine"]);
  });

  it("treats a failed or disabled fetch as not loaded, so nothing gets removed", async () => {
    vi.useFakeTimers();
    const store = useCanvasStore();
    backend.listCanvasInstances.mockRejectedValueOnce(new Error("backend not ready"));
    await store.refresh("s1");
    expect(store.isLoaded("s1")).toBe(false);
    backend.listCanvasInstances.mockResolvedValueOnce({ enabled: false, instances: [], available: [] });
    await vi.advanceTimersByTimeAsync(600);
    expect(store.isLoaded("s1")).toBe(false);
    backend.listCanvasInstances.mockResolvedValueOnce({ enabled: true, instances: [instance("s1", "a", { visible: false })], available: [] });
    await store.refresh("s1");
    expect(store.isLoaded("s1")).toBe(true);
    expect(store.listFor("s1").map((item) => item.instanceId)).toEqual(["a"]);
    vi.useRealTimers();
  });

  it("keeps a few hidden canvases connected and drops the rest", () => {
    const store = useCanvasStore();
    const open = (id: string, view: string) => {
      store.setViewActive(view, true);
      store.claim("s1", id, view);
      store.setLive("s1", id, { url: `http://127.0.0.1:1/${id}/`, title: id, frameKey: 1, theme: "dark" });
    };
    // 前台显示中的不计入后台预算
    open("front", "v-front");
    for (const [index, id] of ["a", "b", "c", "d", "e"].entries()) {
      open(id, `v-${id}`);
      store.setViewActive(`v-${id}`, false);
      store.pruneLive(1_000 + index);
    }
    store.pruneLive(2_000);
    const keys = Object.keys(store.live).sort();
    expect(keys).toContain("s1/front");
    // 最多保留 MAX_BACKGROUND_FRAMES 个后台页面，最近藏起来的优先
    expect(keys.filter((key) => key !== "s1/front")).toHaveLength(MAX_BACKGROUND_FRAMES);
    expect(keys).toContain("s1/e");
    expect(keys).not.toContain("s1/a");
    // 后台太久的也卸掉
    store.pruneLive(2_000 + BACKGROUND_TTL_MS + 10_000);
    expect(Object.keys(store.live)).toEqual(["s1/front"]);
  });

  it("closing a canvas drops its live page", async () => {
    const store = useCanvasStore();
    store.setViewActive("v", true);
    store.claim("s1", "a", "v");
    store.setLive("s1", "a", { url: "http://127.0.0.1:1/a/", title: "A", frameKey: 1, theme: "dark" });
    await store.close("s1", "a");
    expect(store.live["s1/a"]).toBeUndefined();
  });

  it("registers as a host so the chat panel knows to use a modal otherwise", () => {
    const store = useCanvasStore();
    expect(store.hosts).toBe(0);
    const release = store.registerHost();
    expect(store.hosts).toBe(1);
    release();
    expect(store.hosts).toBe(0);
  });
});

describe("fillComposerFromUi", () => {
  it("only fills an empty composer and never sends", () => {
    expect(fillComposerFromUi({ sessionId: "none", source: "canvas", sourceId: "a", text: "hi" })).toEqual({
      ok: false,
      reason: "session_closed",
    });
    let draft = "";
    const unregister = registerComposerFiller("s1", (text) => {
      if (draft) return { ok: false, reason: "composer_not_empty" };
      draft = text;
      return { ok: true };
    });
    expect(fillComposerFromUi({ sessionId: "s1", source: "canvas", sourceId: "a", text: "hi" })).toEqual({ ok: true });
    expect(draft).toBe("hi");
    expect(fillComposerFromUi({ sessionId: "s1", source: "canvas", sourceId: "a", text: "again" })).toEqual({
      ok: false,
      reason: "composer_not_empty",
    });
    unregister();
    expect(fillComposerFromUi({ sessionId: "s1", source: "canvas", sourceId: "a", text: "x" }).ok).toBe(false);
  });
});
