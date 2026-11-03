// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { Workspace } from "../../domain/types";
import { inboxProjectIds, workspacesForProjects } from "./inbox-workspaces";

function workspace(id: string, projectId: string): Workspace {
  return {
    id,
    projectId,
    name: id,
    kind: "folder",
    sourcePath: "/",
    workPath: "/",
    branch: null,
    isolationMode: "direct",
    createdAt: "",
  };
}

describe("inbox workspaces", () => {
  it("prefers the session project list over the inbox item's first project", () => {
    expect(inboxProjectIds({
      itemProjectId: "a",
      sessionProjectId: "a",
      sessionProjectIds: ["a", "b"],
    })).toEqual(["a", "b"]);
  });

  it("falls back to the session project, then the inbox item", () => {
    expect(inboxProjectIds({ itemProjectId: "a", sessionProjectId: "b" })).toEqual(["b"]);
    expect(inboxProjectIds({ itemProjectId: "a" })).toEqual(["a"]);
    expect(inboxProjectIds({})).toEqual([]);
  });

  it("keeps store order and drops other projects", () => {
    const list = [workspace("1", "b"), workspace("2", "a"), workspace("3", "c")];
    expect(workspacesForProjects(["a", "b"], list).map((item) => item.id)).toEqual(["1", "2"]);
    expect(workspacesForProjects([], list)).toEqual([]);
  });
});
