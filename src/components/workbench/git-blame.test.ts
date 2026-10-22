// @vitest-environment node
import { beforeAll, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import { blameAgo, blameEditorText, blameStatusText, type GitBlameInfo } from "./git-blame";

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

  test("uses the default editor and status bar templates", () => {
    expect(blameEditorText(info, now)).toBe("修正边栏, Ada (3 天前)");
    expect(blameStatusText(info, now)).toBe("Ada (3 天前)");
  });
});
