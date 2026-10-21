import { describe, expect, it } from "vitest";
import type { GlobalResource } from "../domain/types";
import { highlightParts, resourceGroups } from "./resource-text";

const item = (owner: string, name: string, scope: "global" | "workspace" = "global"): GlobalResource => ({
  id: `skill:${owner}:${name}`,
  kind: "skill",
  owner,
  scope,
  sourceLabel: scope === "workspace" ? "项目 · skills" : owner,
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

describe("resource text", () => {
  it("highlights a query inside the name", () => {
    expect(highlightParts("pdf-helper", "pdf")).toEqual(["", "pdf", "-helper"]);
  });

  it("puts workspace groups first", () => {
    const groups = resourceGroups([
      item("multara", "a"),
      item("ws/1/github", "b", "workspace"),
    ]);
    expect(groups.map((group) => group.scope)).toEqual(["workspace", "global"]);
  });
});
