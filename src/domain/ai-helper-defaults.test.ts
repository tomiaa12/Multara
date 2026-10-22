import { beforeAll, describe, expect, it } from "vitest";
import { applyLanguageSetting } from "../i18n";
import {
  defaultBranchPrompt,
  defaultCommitPrompt,
  displayPrompt,
  normalizeGenMode,
  persistPrompt,
} from "./ai-helper-defaults";

beforeAll(() => applyLanguageSetting("zh-CN"));

describe("ai-helper-defaults", () => {
  it("normalizes generator mode", () => {
    expect(normalizeGenMode("api")).toBe("api");
    expect(normalizeGenMode("agent")).toBe("agent");
    expect(normalizeGenMode("")).toBe("agent");
    expect(normalizeGenMode(undefined)).toBe("agent");
  });

  it("shows default prompt when stored value is empty", () => {
    const branch = defaultBranchPrompt();
    const commit = defaultCommitPrompt();
    expect(displayPrompt("", branch)).toBe(branch);
    expect(displayPrompt("  ", commit)).toBe(commit);
    expect(displayPrompt("自定义", branch)).toBe("自定义");
  });

  it("persists empty string for default prompt text", () => {
    const branch = defaultBranchPrompt();
    const commit = defaultCommitPrompt();
    expect(persistPrompt(branch, branch)).toBe("");
    expect(persistPrompt(`  ${commit}  `, commit)).toBe("");
    expect(persistPrompt("自定义规则", branch)).toBe("自定义规则");
  });

  it("localizes default prompts", () => {
    applyLanguageSetting("en");
    expect(defaultCommitPrompt()).toContain("Conventional Commit");
    applyLanguageSetting("zh-CN");
    expect(defaultCommitPrompt()).toContain("已暂存");
  });
});
