import { describe, expect, it } from "vitest";
import type { GlobalResource, Workspace } from "../domain/types";
import {
  ALL_SOURCES,
  buildResourceSourceNav,
  compareResourceSourceLabels,
  matchesResourceSource,
  parseWorkspaceOwner,
} from "./resource-source-nav";

const skill = (owner: string, name: string, sourceLabel = ""): GlobalResource => ({
  id: `skill:${owner}:${name}`,
  kind: "skill",
  owner,
  scope: owner.startsWith("ws/") ? "workspace" : "global",
  sourceLabel,
  name,
  description: "",
  sourcePath: "",
  enabled: true,
  updatedAt: "",
  transport: "",
  summary: "",
  needsAuth: false,
  configStatus: "",
});

describe("parseWorkspaceOwner", () => {
  it("parses ws owners", () => {
    expect(parseWorkspaceOwner("ws/abc/cursor")).toEqual({ workspaceId: "abc", key: "cursor" });
    expect(parseWorkspaceOwner("cursor")).toBeNull();
  });
});

describe("matchesResourceSource", () => {
  it("matches all / exact / project prefix", () => {
    expect(matchesResourceSource("cursor", ALL_SOURCES)).toBe(true);
    expect(matchesResourceSource("ws/a/cursor", "ws/a/cursor")).toBe(true);
    expect(matchesResourceSource("ws/a/cursor", "ws/a")).toBe(true);
    expect(matchesResourceSource("ws/a/agents", "ws/a")).toBe(true);
    expect(matchesResourceSource("ws/b/cursor", "ws/a")).toBe(false);
    expect(matchesResourceSource("cursor", "ws/a")).toBe(false);
  });
});

describe("compareResourceSourceLabels", () => {
  it("pins Multara before other sources", () => {
    const rows = [
      { id: "cursor", label: "Cursor" },
      { id: "multara", label: "Multara" },
      { id: "claude", label: "Claude Code" },
    ].sort(compareResourceSourceLabels);
    expect(rows.map((item) => item.id)).toEqual(["multara", "claude", "cursor"]);
  });
});

describe("buildResourceSourceNav", () => {
  it("groups workspace paths under project name", () => {
    const workspaces: Workspace[] = [{
      id: "w1",
      projectId: "p1",
      name: "uc",
      kind: "git",
      sourcePath: "/tmp/uc",
      workPath: "/tmp/uc",
      branch: "main",
      isolationMode: "direct",
      createdAt: "",
    }];
    const { groups, knownIds } = buildResourceSourceNav({
      resources: [
        skill("cursor", "a"),
        skill("multara", "m"),
        skill("ws/w1/cursor", "b", "uc · .cursor/skills"),
        skill("ws/w1/agents", "c", "uc · .agents/skills"),
      ],
      workspaces,
      kind: "skill",
      activeSource: "ws/w1",
      globalLabel: (owner) => {
        if (owner === "cursor") return "Cursor";
        if (owner === "multara") return "Multara";
        return owner;
      },
    });
    expect(groups[0]?.items.map((item) => item.id)).toEqual([ALL_SOURCES, "multara", "cursor"]);
    const project = groups[1]?.items[0];
    expect(project?.id).toBe("ws/w1");
    expect(project?.label).toBe("uc");
    expect(project?.active).toBe(true);
    expect(project?.children?.map((item) => item.label)).toEqual([".agents/skills", ".cursor/skills"]);
    expect(knownIds).toContain("ws/w1/cursor");
  });
});
