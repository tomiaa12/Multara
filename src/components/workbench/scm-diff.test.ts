// @vitest-environment node
import { beforeEach, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import { loadScmDiff, scmDiffPlan } from "./scm-diff";

beforeEach(() => {
  applyLanguageSetting("zh-CN");
});

describe("scmDiffPlan", () => {
  test("compares staged files with HEAD and the index", () => {
    expect(scmDiffPlan({ group: "staged" })).toMatchObject({
      original: "head",
      modified: "index",
      short: "暂存",
    });
  });

  test("compares worktree files with the index", () => {
    expect(scmDiffPlan({ group: "changes" })).toMatchObject({
      original: "index",
      modified: "worktree",
    });
    expect(scmDiffPlan({ group: "merge" })).toMatchObject({
      original: "head",
      modified: "worktree",
      short: "合并",
    });
  });

  test("compares untracked files with an empty document", () => {
    expect(scmDiffPlan({ group: "untracked" })).toMatchObject({
      original: "empty",
      modified: "worktree",
    });
  });

  test("compares a commit file with its parent", () => {
    expect(scmDiffPlan({ group: "commit", commit: "abc", parent: "def", status: "M" })).toMatchObject({
      original: { rev: "def" },
      modified: { rev: "abc" },
    });
    expect(scmDiffPlan({ group: "commit", commit: "abc", parent: "def", status: "A" }).original).toBe("empty");
    expect(scmDiffPlan({ group: "commit", commit: "abc", parent: "def", status: "D" }).modified).toBe("empty");
  });
});

describe("loadScmDiff", () => {
  test("uses an empty modified side when the worktree file is gone", async () => {
    const loaded = await loadScmDiff(
      { workspaceId: "w", path: "gone.ts", group: "changes" },
      {
        readWorktree: async () => null,
        readIndex: async () => ({ content: "old\n", binary: false }),
        readRevision: async () => ({ content: "", binary: false }),
      },
    );
    expect(loaded.original).toBe("old\n");
    expect(loaded.modified).toBe("");
    expect(loaded.language).toBe("typescript");
    expect(loaded.binary).toBe(false);
  });
});
