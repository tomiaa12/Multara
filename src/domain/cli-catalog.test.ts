import { describe, expect, test } from "vitest";
import { CLI_CATALOG, cliEntry, cliIconProvider, cliLabel, cliMcpDisplay, cliSkillsDisplay, runtimeEntries, skillSourceEntries } from "./cli-catalog";
import { RUNTIME_FAMILIES } from "../views/runtime-catalog";

describe("cli-catalog", () => {
  test("shared catalog covers runtime families", () => {
    expect(CLI_CATALOG.length).toBeGreaterThan(10);
    expect(runtimeEntries().map((item) => item.id)).toEqual(RUNTIME_FAMILIES.map((item) => item.id));
  });

  test("skill sources include known homes and skip agents-only runtimes", () => {
    expect(skillSourceEntries().map((item) => item.id)).toContain("dsh");
    expect(skillSourceEntries().map((item) => item.id)).toContain("opencode");
    expect(skillSourceEntries().map((item) => item.id)).toContain("kimi");
    expect(skillSourceEntries().map((item) => item.id)).toContain("deveco");
    expect(cliSkillsDisplay("dsh")).toBe("~/.dsh/skills");
    expect(cliMcpDisplay("cursor")).toBe("~/.cursor/mcp.json");
    expect(cliLabel("cursor")).toBe("Cursor");
    expect(cliIconProvider("qoderclicn")).toBe("qoder");
    expect(cliIconProvider("ws/demo/cursor")).toBe("cursor");
    expect(cliIconProvider("all")).toBe("all");
    const missingSkills = runtimeEntries().filter((item) => !item.skillsHome).map((item) => item.id);
    expect(missingSkills).toEqual(["dim", "qwenpaw"]);
  });

  test("runtime fields stay complete", () => {
    for (const item of runtimeEntries()) {
      expect(item.command).toBeTruthy();
      expect(item.protocol).toBeTruthy();
      expect(cliEntry(item.id)?.runtime).toBe(true);
      expect(item.mcpHome || item.mcpSpecial).toBeTruthy();
    }
  });
});
