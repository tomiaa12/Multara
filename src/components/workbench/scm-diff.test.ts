// @vitest-environment node
import { beforeEach, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import {
  isBinaryWorktree,
  isMediaPath,
  loadScmDiff,
  scmCommitDiffTabName,
  scmDiffPlan,
  scmOpensAsFile,
} from "./scm-diff";

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

  test("opens untracked files and merge conflicts as the file itself", () => {
    expect(scmOpensAsFile("untracked")).toBe(true);
    expect(scmOpensAsFile("merge")).toBe(true);
    expect(scmOpensAsFile("changes")).toBe(false);
    expect(scmOpensAsFile("staged")).toBe(false);
    expect(scmOpensAsFile("commit")).toBe(false);
  });

  test("compares untracked files with an empty document", () => {
    expect(scmDiffPlan({ group: "untracked" })).toMatchObject({
      original: "empty",
      modified: "worktree",
    });
  });

  test("compares a commit file with its parent", () => {
    expect(scmDiffPlan({ group: "commit", commit: "abc", parent: "def", status: "M", path: "a.ts" })).toMatchObject({
      original: { rev: "def" },
      modified: { rev: "abc" },
      tabName: "a.ts (def) ↔ a.ts (abc)",
    });
    expect(scmDiffPlan({ group: "commit", commit: "abc", parent: "def", status: "A" }).original).toBe("empty");
    expect(scmDiffPlan({ group: "commit", commit: "abc", parent: "def", status: "D" }).modified).toBe("empty");
  });
});

describe("scmCommitDiffTabName", () => {
  test("matches history diff titles", () => {
    expect(
      scmCommitDiffTabName({
        path: "src/foo.ts",
        commit: "abcdef012345",
        parent: "1234567890ab",
        status: "M",
      }),
    ).toBe("foo.ts (1234567) ↔ foo.ts (abcdef0)");
    expect(
      scmCommitDiffTabName({ path: "src/foo.ts", commit: "abcdef012345", parent: "1234567890ab", status: "A" }),
    ).toBe("foo.ts (abcdef0)");
    expect(
      scmCommitDiffTabName({ path: "src/foo.ts", commit: "abcdef012345", parent: "1234567890ab", status: "D" }),
    ).toBe("foo.ts (1234567)");
  });
});

describe("isBinaryWorktree", () => {
  test("treats media previews and data URLs as binary", () => {
    expect(isBinaryWorktree({ content: "hi", media: "image" })).toBe(true);
    expect(isBinaryWorktree({ content: "data:image/png;base64,aaa" })).toBe(true);
    expect(isBinaryWorktree({ content: "hello\n" })).toBe(false);
  });
});

describe("isMediaPath", () => {
  test("detects common media extensions", () => {
    expect(isMediaPath("shot.png")).toBe(true);
    expect(isMediaPath("a/b/photo.JPEG")).toBe(true);
    expect(isMediaPath("src/app.ts")).toBe(false);
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

  test("does not open image worktree content as a text diff", async () => {
    const dataUrl = `data:image/png;base64,${"A".repeat(2000)}`;
    const loaded = await loadScmDiff(
      { workspaceId: "w", path: "shot.png", group: "changes" },
      {
        readWorktree: async () => ({ content: dataUrl, language: "plaintext", media: "image" }),
        readIndex: async () => ({ content: "", binary: false }),
        readRevision: async () => ({ content: "", binary: false }),
      },
    );
    expect(loaded.binary).toBe(true);
    expect(loaded.original).toBe("");
    expect(loaded.modified).toBe("");
  });
});
