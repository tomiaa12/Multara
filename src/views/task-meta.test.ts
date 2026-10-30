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
    localStorage.setItem(KEY, JSON.stringify({ a: { priority: "high", labels: ["x"] } }));
    const parse = vi.spyOn(JSON, "parse");
    expect(readTaskMeta("a")).toEqual({ priority: "high", labels: ["x"] });
    expect(readTaskMeta("b")).toEqual({ priority: "none", labels: [] });
    expect(readTaskMeta("a").priority).toBe("high");
    expect(parse).toHaveBeenCalledTimes(1);
  });

  it("updates the cache and storage on write", () => {
    writeTaskMeta("a", { priority: "urgent" });
    writeTaskMeta("a", { labels: ["tag"] });
    expect(readTaskMeta("a")).toEqual({ priority: "urgent", labels: ["tag"] });
    resetTaskMetaCache();
    expect(readTaskMeta("a")).toEqual({ priority: "urgent", labels: ["tag"] });
  });

  it("keeps the in-memory value when storage rejects the write", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceededError");
    });
    expect(() => writeTaskMeta("a", { priority: "low" })).not.toThrow();
    expect(readTaskMeta("a").priority).toBe("low");
  });
});
