import { describe, expect, test } from "vitest";
import { gitStatusForWorkspaces } from "./git-status-scope";

describe("gitStatusForWorkspaces", () => {
  const repositories = [
    { workspaceId: "feature", changes: 0 },
    { workspaceId: "merge", changes: 182 },
  ];

  test("drops a merge worktree that is not on the current page", () => {
    expect(gitStatusForWorkspaces(repositories, ["feature"]).map((item) => item.workspaceId)).toEqual(["feature"]);
  });

  test("keeps the merge worktree when that session is the page being shown", () => {
    expect(gitStatusForWorkspaces(repositories, ["merge"]).map((item) => item.workspaceId)).toEqual(["merge"]);
  });

  test("returns nothing when the page has no workspaces", () => {
    expect(gitStatusForWorkspaces(repositories, [])).toEqual([]);
  });
});
