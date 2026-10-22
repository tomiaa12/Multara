import { beforeAll, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import { formatAbsoluteTime, formatRelativeTime, parseTimeMs } from "./relative-time";

describe("formatRelativeTime", () => {
  beforeAll(() => applyLanguageSetting("zh-CN"));
  const now = Date.parse("2026-10-20T12:00:00.000Z");

  test("formats minute hour day month year", () => {
    expect(formatRelativeTime("2026-10-20T11:59:30.000Z", now)).toBe("刚刚");
    expect(formatRelativeTime("2026-10-20T11:57:00.000Z", now)).toBe("3 分钟前");
    expect(formatRelativeTime("2026-10-20T09:00:00.000Z", now)).toBe("3 小时前");
    expect(formatRelativeTime("2026-10-17T12:00:00.000Z", now)).toBe("3 天前");
    expect(formatRelativeTime("2026-07-20T12:00:00.000Z", now)).toBe("3 个月前");
    expect(formatRelativeTime("2024-10-20T12:00:00.000Z", now)).toBe("2 年前");
  });

  test("handles empty input", () => {
    expect(formatRelativeTime(null, now)).toBe("");
    expect(formatRelativeTime("", now)).toBe("");
    expect(parseTimeMs("not-a-date")).toBeNaN();
    expect(formatAbsoluteTime("2026-10-20T12:00:00.000Z")).toBeTruthy();
  });
});
