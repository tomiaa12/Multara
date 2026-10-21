import { describe, expect, it } from "vitest";
import {
  DEFAULT_BRANCH_PROMPT,
  DEFAULT_COMMIT_PROMPT,
  displayPrompt,
  normalizeGenMode,
  persistPrompt,
} from "./ai-helper-defaults";

describe("ai-helper-defaults", () => {
  it("normalizes generator mode", () => {
    expect(normalizeGenMode("api")).toBe("api");
    expect(normalizeGenMode("agent")).toBe("agent");
    expect(normalizeGenMode("")).toBe("agent");
    expect(normalizeGenMode(undefined)).toBe("agent");
  });

  it("shows default prompt when stored value is empty", () => {
    expect(displayPrompt("", DEFAULT_BRANCH_PROMPT)).toBe(DEFAULT_BRANCH_PROMPT);
    expect(displayPrompt("  ", DEFAULT_COMMIT_PROMPT)).toBe(DEFAULT_COMMIT_PROMPT);
    expect(displayPrompt("自定义", DEFAULT_BRANCH_PROMPT)).toBe("自定义");
  });

  it("persists empty string for default prompt text", () => {
    expect(persistPrompt(DEFAULT_BRANCH_PROMPT, DEFAULT_BRANCH_PROMPT)).toBe("");
    expect(persistPrompt(`  ${DEFAULT_COMMIT_PROMPT}  `, DEFAULT_COMMIT_PROMPT)).toBe("");
    expect(persistPrompt("自定义规则", DEFAULT_BRANCH_PROMPT)).toBe("自定义规则");
  });
});
