import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

type Model = { getValue(): string; setValue(value: string): void; dispose(): void };
const mocks = vi.hoisted(() => {
  const models = new Map<string, Model>();
  return {
    models,
    collectSourceFiles: vi.fn(),
    collectWorkspaceTypings: vi.fn(),
    setExtraLibs: vi.fn(),
    createModel: vi.fn((content: string, _language: unknown, uri: { path: string }) => {
      let value = content;
      const model = {
        getValue: () => value,
        setValue: (next: string) => { value = next; },
        dispose: () => { models.delete(uri.path); },
      };
      models.set(uri.path, model);
      return model;
    }),
  };
});

vi.mock("monaco-editor", () => ({
  typescript: {
    typescriptDefaults: { setExtraLibs: mocks.setExtraLibs },
    javascriptDefaults: { setExtraLibs: mocks.setExtraLibs },
  },
}));
vi.mock("../monaco", () => ({
  monaco: {
    editor: {
      createModel: mocks.createModel,
      getModel: (uri: { path: string }) => mocks.models.get(uri.path),
      getEditors: () => [],
    },
  },
}));
vi.mock("../services/backend", () => ({
  collectSourceFiles: mocks.collectSourceFiles,
  collectWorkspaceTypings: mocks.collectWorkspaceTypings,
}));
vi.mock("./workspace-models", () => ({
  workspaceModelUri: (workspaceId: string, path: string) => ({ path: `/${workspaceId}/${path}` }),
}));
vi.mock("./monaco-workspace", () => ({ applyWorkspaceTypescript: vi.fn() }));

const MiB = 1024 * 1024;
let codeIndex: (typeof import("./code-index"))["codeIndex"];

beforeEach(async () => {
  vi.useFakeTimers();
  vi.resetModules();
  vi.clearAllMocks();
  mocks.models.clear();
  mocks.collectSourceFiles.mockReset().mockResolvedValue({ files: [], truncated: false });
  mocks.collectWorkspaceTypings.mockReset().mockResolvedValue([]);
  codeIndex = (await import("./code-index")).codeIndex;
});

afterEach(() => {
  vi.useRealTimers();
});

function source(path: string, content: string | null, mtime = 1) {
  return { path, content, mtime };
}

describe("workspace code index memory limits", () => {
  test("shares the source budget across workspaces and frees it on release", async () => {
    const content = "x".repeat(512 * 1024);
    mocks.collectSourceFiles.mockImplementation(async (workspaceId: string) => ({
      files: Array.from({ length: 32 }, (_, index) => source(`${workspaceId}-${index}.ts`, content)),
      truncated: false,
    }));
    const owners = [{}, {}, {}];
    codeIndex.setWorkspaces(owners[0]!, ["a"]);
    codeIndex.setWorkspaces(owners[1]!, ["b"]);
    codeIndex.setWorkspaces(owners[2]!, ["c"]);
    await vi.runAllTimersAsync();
    expect(mocks.models.size).toBe(64);
    expect([...mocks.models.values()].reduce((bytes, model) => bytes + model.getValue().length, 0)).toBe(32 * MiB);
    expect([...mocks.models.keys()].some((path) => path.startsWith("/c/"))).toBe(false);
    codeIndex.release(owners[0]!);
    codeIndex.refreshAll();
    await vi.runAllTimersAsync();
    expect(mocks.models.size).toBe(64);
    expect([...mocks.models.keys()].some((path) => path.startsWith("/c/"))).toBe(true);
  });

  test("limits model count even for tiny sources", async () => {
    mocks.collectSourceFiles.mockResolvedValue({
      files: Array.from({ length: 3_001 }, (_, index) => source(`${index}.ts`, "x")),
      truncated: false,
    });
    codeIndex.setWorkspaces({}, ["a"]);
    await vi.runAllTimersAsync();
    expect(mocks.models.size).toBe(3_000);
  });

  test("reconciles incremental results without overwriting open editors", async () => {
    mocks.collectSourceFiles.mockResolvedValueOnce({
      files: [source("a.ts", "old"), source("removed.ts", "remove"), source("open.ts", "on disk")],
      truncated: false,
    });
    codeIndex.setWorkspaces({}, ["a"]);
    await vi.runAllTimersAsync();
    codeIndex.registerOpenChecker((_workspaceId, path) => path === "open.ts");
    mocks.models.get("/a/open.ts")!.setValue("unsaved edits");
    mocks.collectSourceFiles.mockResolvedValueOnce({
      files: [source("a.ts", null), source("open.ts", "changed on disk", 2)],
      truncated: false,
    });
    codeIndex.refreshAll();
    await vi.runAllTimersAsync();
    expect(mocks.collectSourceFiles).toHaveBeenLastCalledWith("a", expect.any(Array), {
      "a.ts": 1, "removed.ts": 1, "open.ts": 1,
    });
    expect(mocks.models.has("/a/removed.ts")).toBe(false);
    expect(mocks.models.get("/a/a.ts")!.getValue()).toBe("old");
    expect(mocks.models.get("/a/open.ts")!.getValue()).toBe("unsaved edits");
  });

  test("does not recreate models after release during a chunked refresh", async () => {
    mocks.collectSourceFiles.mockResolvedValue({
      files: Array.from({ length: 150 }, (_, index) => source(`${index}.ts`, "x")),
      truncated: false,
    });
    const owner = {};
    codeIndex.setWorkspaces(owner, ["a"]);
    await Promise.resolve();
    await Promise.resolve();
    expect(mocks.models.size).toBe(50);
    codeIndex.release(owner);
    await vi.runAllTimersAsync();
    expect(mocks.models.size).toBe(0);
  });

  test("shares the declaration budget across workspaces", async () => {
    const content = "x".repeat(2 * MiB);
    mocks.collectWorkspaceTypings.mockImplementation(async () =>
      Array.from({ length: 12 }, (_, index) => ({ path: `node_modules/types/${index}.d.ts`, content, diskPath: null })),
    );
    codeIndex.setWorkspaces({}, ["a", "b"]);
    await vi.runAllTimersAsync();
    const libs = mocks.setExtraLibs.mock.lastCall![0] as Array<{ content: string }>;
    expect(libs).toHaveLength(16);
    expect(libs.reduce((bytes, lib) => bytes + lib.content.length, 0)).toBe(32 * MiB);
  });

  test("drops stale declarations when a workspace is removed and re-added", async () => {
    let resolveOld!: (value: unknown) => void;
    mocks.collectWorkspaceTypings.mockReturnValueOnce(new Promise((resolve) => { resolveOld = resolve; }));
    const owner = {};
    codeIndex.setWorkspaces(owner, ["a"]);
    codeIndex.release(owner);
    codeIndex.setWorkspaces(owner, ["a"]);
    await vi.runAllTimersAsync();
    resolveOld([{ path: "node_modules/stale.d.ts", content: "export {};", diskPath: null }]);
    await vi.runAllTimersAsync();
    expect(mocks.setExtraLibs.mock.lastCall![0]).toEqual([]);
  });
});
