// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readTaskMeta, resetTaskMetaCache, writeTaskMeta } from "./task-meta";

const KEY = "multara.task-meta";

describe("task meta cache", () => {
  beforeEach(() => {
    localStorage.clear();
    resetTaskMetaCache();
    vi.restoreAllMocks();
  });

  it("parses storage once and serves later reads from the cache", () => {
    localStorage.setItem(KEY, JSON.stringify({ a: { labels: ["x"] } }));
    const parse = vi.spyOn(JSON, "parse");
    expect(readTaskMeta("a")).toEqual({ labels: ["x"] });
    expect(readTaskMeta("b")).toEqual({ labels: [] });
    expect(readTaskMeta("a").labels).toEqual(["x"]);
    expect(parse).toHaveBeenCalledTimes(1);
  });

  it("updates the cache and storage on write", () => {
    writeTaskMeta("a", { labels: ["tag"] });
    expect(readTaskMeta("a")).toEqual({ labels: ["tag"] });
    resetTaskMetaCache();
    expect(readTaskMeta("a")).toEqual({ labels: ["tag"] });
  });

  it("keeps the in-memory value when storage rejects the write", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceededError");
    });
    expect(() => writeTaskMeta("a", { labels: ["kept"] })).not.toThrow();
    expect(readTaskMeta("a").labels).toEqual(["kept"]);
  });
});
