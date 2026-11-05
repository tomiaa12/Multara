// @vitest-environment node
import { beforeAll, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import { md5Hex } from "../../utils/md5";
import { gravatarUrl } from "../../utils/gravatar";
import {
  blameAgo,
  blameEditorText,
  blameLineDiffSnippet,
  blameStatusText,
  remoteCommitWebUrl,
  type GitBlameInfo,
} from "./git-blame";
import { mapCurrentLineToBlameLine } from "./git-blame-map";

beforeAll(() => applyLanguageSetting("zh-CN"));

const now = Date.parse("2026-09-26T02:00:00Z");

const info: GitBlameInfo = {
  committed: true,
  hash: "abcdef1234567890",
  authorName: "Ada",
  authorEmail: "ada@example.com",
  authorTime: Math.floor(Date.parse("2026-09-23T02:00:00Z") / 1000),
  subject: "修正边栏",
};

describe("git blame text", () => {
  test("formats relative time the way the blame decoration does", () => {
    expect(blameAgo(Math.floor(now / 1000) - 30, now)).toBe("刚刚");
    expect(blameAgo(Math.floor(now / 1000) - 120, now)).toBe("2 分钟前");
    expect(blameAgo(Math.floor(now / 1000) - 3 * 3600, now)).toBe("3 小时前");
    expect(blameAgo(info.authorTime, now)).toBe("3 天前");
  });

  test("uses the GitLens-style editor template", () => {
    expect(blameEditorText(info, now)).toBe("Ada, 3 天前 • 修正边栏");
    expect(blameStatusText(info, now)).toBe("Ada (3 天前)");
  });

  test("formats uncommitted lines like GitLens", () => {
    const dirty: GitBlameInfo = {
      committed: false,
      hash: "",
      authorName: "",
      authorEmail: "",
      authorTime: Math.floor(now / 1000) - 60 * 16,
      subject: "",
    };
    expect(blameEditorText(dirty, now)).toBe("你, 16 分钟前 • 未提交的更改");
  });
});

describe("mapCurrentLineToBlameLine", () => {
  test("maps unchanged and modified lines, and nulls pure additions", () => {
    const base = ["A", "B", "C"].join("\n");
    const current = ["A", "B2", "EXTRA", "C"].join("\n");
    expect(mapCurrentLineToBlameLine(base, current, 1)).toBe(1);
    expect(mapCurrentLineToBlameLine(base, current, 2)).toBe(2);
    expect(mapCurrentLineToBlameLine(base, current, 3)).toBeNull();
    expect(mapCurrentLineToBlameLine(base, current, 4)).toBe(3);
  });
});

describe("blameLineDiffSnippet", () => {
  test("returns only the current line from a multi-line hunk", () => {
    const oldText = ["a", "old1", "old2", "c"].join("\n");
    const newText = ["a", "new1", "new2", "c"].join("\n");
    expect(blameLineDiffSnippet(oldText, newText, 2)).toEqual([
      { kind: "deleted", text: "old1" },
      { kind: "added", text: "new1" },
    ]);
    expect(blameLineDiffSnippet(oldText, newText, 3)).toEqual([
      { kind: "deleted", text: "old2" },
      { kind: "added", text: "new2" },
    ]);
  });
});

describe("gravatar and remote urls", () => {
  test("builds a gravatar url from email md5", () => {
    expect(md5Hex("ada@example.com")).toMatch(/^[0-9a-f]{32}$/);
    expect(gravatarUrl("Ada@Example.com", 40)).toContain(md5Hex("ada@example.com"));
  });

  test("builds commit web urls for common hosts", () => {
    expect(remoteCommitWebUrl("git@github.com:acme/app.git", "abc123")).toBe(
      "https://github.com/acme/app/commit/abc123",
    );
    expect(remoteCommitWebUrl("https://gitlab.com/acme/app.git", "abc123")).toBe(
      "https://gitlab.com/acme/app/-/commit/abc123",
    );
  });
});
