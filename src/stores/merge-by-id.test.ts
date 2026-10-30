import { describe, expect, it } from "vitest";
import { mergeById } from "./merge-by-id";

describe("mergeById", () => {
  it("returns the previous array when nothing changed", () => {
    const previous = [{ id: "a", tags: ["x"] }, { id: "b", tags: [] }];
    const next = [{ id: "a", tags: ["x"] }, { id: "b", tags: [] }];
    expect(mergeById(previous, next)).toBe(previous);
  });

  it("keeps unchanged item references and takes changed ones", () => {
    const previous = [{ id: "a", title: "A" }, { id: "b", title: "B" }];
    const merged = mergeById(previous, [{ id: "b", title: "B" }, { id: "a", title: "A2" }, { id: "c", title: "C" }]);
    expect(merged).not.toBe(previous);
    expect(merged[0]).toBe(previous[1]);
    expect(merged[1]).toEqual({ id: "a", title: "A2" });
    expect(merged.map((item) => item.id)).toEqual(["b", "a", "c"]);
  });

  it("detects removals", () => {
    const previous = [{ id: "a" }, { id: "b" }];
    expect(mergeById(previous, [{ id: "a" }])).toEqual([previous[0]]);
  });
});
