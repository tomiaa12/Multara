import { beforeAll, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import { mergeTimeline, relativeTimelineTime, renameLocalHistoryPath, registerLocalHistory, listLocalHistory } from "./local-history";

beforeAll(() => applyLanguageSetting("zh-CN"));

const now = Date.parse("2026-09-25T12:00:00Z");

describe("relativeTimelineTime", () => {
  test("uses a short relative label", () => {
    expect(relativeTimelineTime(now - 30_000, now)).toBe("刚刚");
    expect(relativeTimelineTime(now - 5 * 60_000, now)).toBe("5 分钟前");
    expect(relativeTimelineTime(now - 3 * 60 * 60_000, now)).toBe("3 小时前");
    expect(relativeTimelineTime(now - 26 * 60 * 60_000, now)).toBe("昨天");
  });
});

describe("mergeTimeline", () => {
  test("orders newest first and hides a repeated timestamp", () => {
    const rows = mergeTimeline([
      { id: "old", source: "git", label: "旧提交", detail: "Ada", at: now - 3 * 60 * 60_000 },
      { id: "save", source: "local", label: "保存", detail: "", at: now - 4 * 60_000 },
      { id: "save-2", source: "local", label: "保存", detail: "", at: now - 4 * 60_000 - 20_000 },
    ], now);
    expect(rows.map((row) => row.id)).toEqual(["save", "save-2", "old"]);
    expect(rows[0].timestamp).toBe("4 分钟前");
    expect(rows[1].hideTimestamp).toBe(true);
    expect(rows[0].hideTimestamp).toBe(false);
  });
});

describe("local history rename tracking", () => {
  test("follows a file after it is renamed", () => {
    registerLocalHistory([
      { id: "rename-test", workspaceId: "w", path: "src/a.ts", savedAt: 1, source: "保存", content: "one" },
    ]);
    renameLocalHistoryPath("w", "src/a.ts", "src/b.ts");
    expect(listLocalHistory("w", "src/b.ts").some((item) => item.id === "rename-test")).toBe(true);
    expect(listLocalHistory("w", "src/a.ts").some((item) => item.id === "rename-test")).toBe(false);
  });
});
