// @vitest-environment node
import { describe, expect, test } from "vitest";
import { gitGutterModel } from "./git-gutter-marks";
import { applyLineChanges, revertLineChange } from "./git-line-changes";

describe("applyLineChanges", () => {
  test("stages one hunk onto baseline and leaves other working-tree edits out", () => {
    const baseline = ["A", "B", "C", "D", "E"].join("\n");
    const current = ["A", "B2", "C", "D2", "E"].join("\n");
    const { changes } = gitGutterModel(baseline, current, 5);
    expect(changes).toHaveLength(2);
    expect(applyLineChanges(baseline, [changes[0]])).toBe(["A", "B2", "C", "D", "E"].join("\n"));
    expect(applyLineChanges(baseline, [changes[1]])).toBe(["A", "B", "C", "D2", "E"].join("\n"));
    expect(applyLineChanges(baseline, changes)).toBe(current);
  });

  test("reverts one hunk and keeps the rest", () => {
    const baseline = ["A", "B", "C", "D", "E"].join("\n");
    const current = ["A", "B2", "C", "D2", "E"].join("\n");
    const { changes } = gitGutterModel(baseline, current, 5);
    expect(revertLineChange(current, changes[0])).toBe(["A", "B", "C", "D2", "E"].join("\n"));
    expect(revertLineChange(current, changes[1])).toBe(["A", "B2", "C", "D", "E"].join("\n"));
  });

  test("handles pure additions and deletions", () => {
    const addedBaseline = ["a", "c"].join("\n");
    const addedCurrent = ["a", "c", "added"].join("\n");
    const { changes: added } = gitGutterModel(addedBaseline, addedCurrent, 3);
    expect(applyLineChanges(addedBaseline, added)).toBe(addedCurrent);
    expect(revertLineChange(addedCurrent, added[0])).toBe(addedBaseline);

    const deletedBaseline = ["a", "b", "c"].join("\n");
    const deletedCurrent = ["a", "c"].join("\n");
    const { changes: deleted } = gitGutterModel(deletedBaseline, deletedCurrent, 2);
    expect(applyLineChanges(deletedBaseline, deleted)).toBe(deletedCurrent);
    expect(revertLineChange(deletedCurrent, deleted[0])).toBe(deletedBaseline);
  });
});

