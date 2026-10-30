// @vitest-environment node
import { describe, expect, test } from "vitest";
import { buildTreeGitIndex } from "./tree-git-index";

const change = (path: string, worktreeStatus: string, indexStatus = " ") => ({ path, worktreeStatus, indexStatus });

describe("buildTreeGitIndex", () => {
  test("indexes files and aggregates every ancestor folder", () => {
    const index = buildTreeGitIndex([
      change("src/a/one.ts", "M"),
      change("src/a/two.ts", "?", "?"),
      change("src/b.ts", "D"),
      change("README.md", "M"),
    ]);
    expect(index.files.get("src/a/one.ts")).toBe("M");
    expect(index.folders.get("src/a")).toEqual({ letter: "U", count: 2 });
    expect(index.folders.get("src")).toEqual({ letter: "U", count: 3 });
    expect(index.folders.has("sr")).toBe(false);
    expect(index.files.get("src/c.ts")).toBeUndefined();
  });

  test("matches an untracked directory reported with a trailing slash", () => {
    const index = buildTreeGitIndex([change("dist/", "?", "?")]);
    expect(index.folders.get("dist")).toEqual({ letter: "U", count: 1 });
  });
});
