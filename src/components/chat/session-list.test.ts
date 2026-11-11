import { describe, expect, test } from "vitest";
import {
  clampSessionListWidth,
  groupSessionList,
  latestActivity,
  sessionListGroupKey,
  sessionListLayout,
} from "./session-list";

const now = new Date(2026, 10, 11, 14, 30);

function at(daysAgo: number, hour = 10) {
  return new Date(2026, 10, 11 - daysAgo, hour).toISOString();
}

describe("sessionListGroupKey", () => {
  test("buckets by calendar day, not by 24h windows", () => {
    expect(sessionListGroupKey(at(0, 0), now)).toBe("today");
    expect(sessionListGroupKey(at(1, 23), now)).toBe("yesterday");
    expect(sessionListGroupKey(at(1, 0), now)).toBe("yesterday");
    expect(sessionListGroupKey(at(6), now)).toBe("week");
    expect(sessionListGroupKey(at(7), now)).toBe("older");
  });

  test("treats unparseable times as older", () => {
    expect(sessionListGroupKey("", now)).toBe("older");
    expect(sessionListGroupKey("not a date", now)).toBe("older");
  });
});

describe("groupSessionList", () => {
  const items = [
    { id: "a", title: "Fix login", activityAt: at(3) },
    { id: "b", title: "Refactor store", activityAt: at(0, 9) },
    { id: "c", title: "Login copy", activityAt: at(0, 12) },
    { id: "d", title: "Old", activityAt: at(30) },
  ];

  test("sorts newest first and drops empty groups", () => {
    const groups = groupSessionList(items, "", now);
    expect(groups.map((group) => group.key)).toEqual(["today", "week", "older"]);
    expect(groups[0].items.map((item) => item.id)).toEqual(["c", "b"]);
  });

  test("filters by title case-insensitively", () => {
    const groups = groupSessionList(items, "  LOGIN ", now);
    expect(groups.flatMap((group) => group.items.map((item) => item.id))).toEqual(["c", "a"]);
  });

  test("keeps input order for equal times", () => {
    const same = [
      { id: "x", title: "x", activityAt: at(0) },
      { id: "y", title: "y", activityAt: at(0) },
    ];
    expect(groupSessionList(same, "", now)[0].items.map((item) => item.id)).toEqual(["x", "y"]);
  });
});

describe("latestActivity", () => {
  test("returns the newest valid time", () => {
    expect(latestActivity(at(2), null, at(0), "bad", undefined)).toBe(at(0));
    expect(latestActivity(null, "")).toBe("");
  });
});

describe("layout", () => {
  test("goes side-by-side only when the chat keeps enough room", () => {
    expect(sessionListLayout(320)).toBe("drawer");
    expect(sessionListLayout(480)).toBe("drawer");
    expect(sessionListLayout(720)).toBe("side");
  });

  test("clamps the side width so the chat keeps its minimum", () => {
    expect(clampSessionListWidth(100, 720)).toBe(168);
    expect(clampSessionListWidth(400, 1200)).toBe(320);
    expect(clampSessionListWidth(300, 600)).toBe(260);
  });
});
