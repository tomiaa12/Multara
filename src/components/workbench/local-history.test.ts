import { afterEach, beforeAll, describe, expect, test, vi } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import { mergeTimeline, relativeTimelineTime, renameLocalHistoryPath, registerLocalHistory, listLocalHistory } from "./local-history";

beforeAll(() => applyLanguageSetting("zh-CN"));

const now = Date.parse("2026-09-25T12:00:00Z");

describe("relativeTimelineTime", () => {
  test("uses a short relative label", () => {
    expect(relativeTimelineTime(now - 30_000, now)).toBe("刚刚");
    expect(relativeTimelineTime(now - 5 * 60_000, now)).toBe("5 分钟前");
    expect(relativeTimelineTime(now - 3 * 60 * 60_000, now)).toBe("3 小时前");
    expect(relativeTimelineTime(now - 26 * 60 * 60_000, now)).toBe("昨天");
  });
});

describe("mergeTimeline", () => {
  test("orders newest first and hides a repeated timestamp", () => {
    const rows = mergeTimeline([
      { id: "old", source: "git", label: "旧提交", detail: "Ada", at: now - 3 * 60 * 60_000 },
      { id: "save", source: "local", label: "保存", detail: "", at: now - 4 * 60_000 },
      { id: "save-2", source: "local", label: "保存", detail: "", at: now - 4 * 60_000 - 20_000 },
    ], now);
    expect(rows.map((row) => row.id)).toEqual(["save", "save-2", "old"]);
    expect(rows[0].timestamp).toBe("4 分钟前");
    expect(rows[1].hideTimestamp).toBe(true);
    expect(rows[0].hideTimestamp).toBe(false);
  });
});

describe("local history rename tracking", () => {
  test("follows a file after it is renamed", () => {
    registerLocalHistory([
      { id: "rename-test", workspaceId: "w", path: "src/a.ts", savedAt: 1, source: "保存", content: "one" },
    ]);
    renameLocalHistoryPath("w", "src/a.ts", "src/b.ts");
    expect(listLocalHistory("w", "src/b.ts").some((item) => item.id === "rename-test")).toBe(true);
    expect(listLocalHistory("w", "src/a.ts").some((item) => item.id === "rename-test")).toBe(false);
  });
});

function memoryStorage(quota = Infinity) {
  const data = new Map<string, string>();
  const storage = {
    reads: 0,
    getItem(key: string) {
      storage.reads += 1;
      return data.get(key) ?? null;
    },
    setItem(key: string, value: string) {
      if (value.length > quota) throw new DOMException("full", "QuotaExceededError");
      data.set(key, value);
    },
  };
  return storage;
}

async function freshModule(storage: ReturnType<typeof memoryStorage>) {
  vi.stubGlobal("localStorage", storage);
  vi.resetModules();
  return import("./local-history");
}

describe("local history storage", () => {
  afterEach(() => vi.unstubAllGlobals());

  test("parses storage once and serves later reads from the cache", async () => {
    const storage = memoryStorage();
    const history = await freshModule(storage);
    history.recordLocalHistory({ workspaceId: "w", path: "a.ts", source: "保存", content: "one" });
    history.recordLocalHistory({ workspaceId: "w", path: "a.ts", source: "保存", content: "two" });
    expect(history.listLocalHistory("w", "a.ts").map((item) => item.content)).toEqual(["two", "one"]);
    expect(storage.reads).toBe(1);
  });

  test("evicts the oldest entries instead of throwing when the quota is full", async () => {
    const storage = memoryStorage(600);
    const history = await freshModule(storage);
    for (let index = 0; index < 10; index += 1) {
      history.recordLocalHistory({
        workspaceId: "w",
        path: `f${index}.ts`,
        source: "保存",
        content: "x".repeat(50),
        savedAt: index + 1,
      });
    }
    expect(history.listLocalHistory("w", "f9.ts")).toHaveLength(1);
    expect(history.listLocalHistory("w", "f0.ts")).toHaveLength(0);
  });

  test("gives up silently and keeps old history when even one entry does not fit", async () => {
    const history = await freshModule(memoryStorage(300));
    history.recordLocalHistory({ workspaceId: "w", path: "small.ts", source: "保存", content: "x", savedAt: 1 });
    expect(() =>
      history.recordLocalHistory({ workspaceId: "w", path: "big.ts", source: "保存", content: "x".repeat(400), savedAt: 2 }),
    ).not.toThrow();
    expect(history.listLocalHistory("w", "small.ts")).toHaveLength(1);
    expect(history.listLocalHistory("w", "big.ts")).toHaveLength(0);
  });

  test("skips obvious secret files", async () => {
    const history = await freshModule(memoryStorage());
    for (const path of [".env", "config/.env.local", "certs/server.pem", "tls.key"]) {
      history.recordLocalHistory({ workspaceId: "w", path, source: "保存", content: "secret" });
      expect(history.listLocalHistory("w", path)).toHaveLength(0);
    }
    expect(history.isSecretHistoryPath("src/env.ts")).toBe(false);
  });
});
