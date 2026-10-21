// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  addRequirementCategory,
  assignRequirementTags,
  parseRequirementCategories,
  projectMatchesTags,
  removeRequirementCategory,
  renameRequirementCategory,
} from "./requirement-categories";

describe("requirement categories", () => {
  it("ignores malformed storage and keeps old single assignments", () => {
    expect(parseRequirementCategories("nope")).toEqual({ categories: [], assignments: {} });
    const parsed = parseRequirementCategories(JSON.stringify({
      categories: [{ id: "cat-1", name: "前端" }],
      assignments: { "project-1": "cat-1", "project-2": ["missing"] },
    }));
    expect(parsed.categories[0]?.color).toBeTruthy();
    expect(parsed.assignments["project-1"]).toEqual(["cat-1"]);
    expect(parsed.assignments["project-2"]).toBeUndefined();
  });

  it("adds, assigns several tags, and removes one", () => {
    const added = addRequirementCategory(
      addRequirementCategory({ categories: [], assignments: {} }, " 前端 ", "cat-1"),
      "后端",
      "cat-2",
    );
    expect(added.categories.map((item) => item.name)).toEqual(["前端", "后端"]);
    const assigned = assignRequirementTags(added, "project-1", ["cat-1", "cat-2", "nope"]);
    expect(assigned.assignments["project-1"]).toEqual(["cat-1", "cat-2"]);
    expect(projectMatchesTags(assigned.assignments["project-1"], ["cat-2"])).toBe(true);
    expect(projectMatchesTags(assigned.assignments["project-1"], ["other"])).toBe(false);
    const removed = removeRequirementCategory(assigned, "cat-1");
    expect(removed.assignments["project-1"]).toEqual(["cat-2"]);
    expect(renameRequirementCategory(removed, "cat-2", "服务端").categories[0]?.name).toBe("服务端");
  });
});
