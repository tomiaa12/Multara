import { describe, expect, it } from "vitest";
import type { GlobalResource } from "../../domain/types";
import { activeSlashQuery, filterSkillSlash, replaceSlashToken } from "./skill-slash";

const skill = (name: string, description = ""): GlobalResource => ({
  id: `skill:multara:${name}`,
  kind: "skill",
  owner: "multara",
  scope: "global",
  sourceLabel: "Multara",
  name,
  description,
  sourcePath: "",
  enabled: true,
  updatedAt: "",
  transport: "",
  summary: "",
  needsAuth: false,
  configStatus: "",
});

describe("skill slash", () => {
  it("reads a trailing slash query", () => {
    expect(activeSlashQuery("/pdf")).toBe("pdf");
    expect(activeSlashQuery("请用 /rev")).toBe("rev");
    expect(activeSlashQuery("/pdf 继续")).toBeNull();
  });

  it("filters skills by query", () => {
    expect(filterSkillSlash([skill("pdf", "PDF"), skill("xlsx")], "pd").map((item) => item.name)).toEqual(["pdf"]);
  });

  it("replaces the slash token in composer nodes", () => {
    const nodes = replaceSlashToken([{ type: "text", value: "请用 /pd" }], "pdf");
    expect(nodes).toEqual([{ type: "text", value: "请用 /pdf " }]);
  });
});
