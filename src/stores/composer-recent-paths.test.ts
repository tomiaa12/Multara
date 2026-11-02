import { beforeEach, describe, expect, test, vi } from "vitest";

const storage = new Map<string, string>();

vi.mock("../utils/safe-storage", () => ({
  readStorage: (key: string) => storage.get(key) ?? null,
  writeStorage: (key: string, value: string) => {
    storage.set(key, value);
    return true;
  },
  removeStorage: (key: string) => {
    storage.delete(key);
  },
}));

describe("composer-recent-paths", () => {
  beforeEach(() => {
    storage.clear();
    vi.resetModules();
  });

  test("isolates by place and workbench project", async () => {
    const {
      listComposerRecent,
      recordComposerRecent,
    } = await import("./composer-recent-paths");

    recordComposerRecent("chat", { path: "/a/chat.ts" });
    recordComposerRecent("workbench", { path: "/a/wb1.ts", workspaceId: "w1" }, "p1");
    recordComposerRecent("workbench", { path: "/a/wb2.ts", workspaceId: "w2" }, "p2");
    recordComposerRecent("task", { path: "/a/task.ts" });

    expect(listComposerRecent("chat").map((item) => item.path)).toEqual(["/a/chat.ts"]);
    expect(listComposerRecent("workbench", "p1").map((item) => item.path)).toEqual(["/a/wb1.ts"]);
    expect(listComposerRecent("workbench", "p2").map((item) => item.path)).toEqual(["/a/wb2.ts"]);
    expect(listComposerRecent("task").map((item) => item.path)).toEqual(["/a/task.ts"]);
  });

  test("moves duplicate to front", async () => {
    const {
      listComposerRecent,
      recordComposerRecent,
    } = await import("./composer-recent-paths");

    recordComposerRecent("inbox", { path: "/a/one.ts", workspaceId: "w" });
    recordComposerRecent("inbox", { path: "/a/two.ts", workspaceId: "w" });
    recordComposerRecent("inbox", { path: "/a/one.ts", workspaceId: "w" });

    expect(listComposerRecent("inbox").map((item) => item.path)).toEqual([
      "/a/one.ts",
      "/a/two.ts",
    ]);
  });
});
