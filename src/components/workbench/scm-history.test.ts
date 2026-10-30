// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  historyCircleColor,
  historyCircleIndex,
  historyColorMap,
  historyGraph,
  historyGraphWidth,
  historyRefBadges,
  historyRefs,
  historyRowGraphWidth,
  isHistoryHead,
  layoutHistory,
  layoutHistoryGraph,
  parseHistoryRef,
  SCM_COLOR_REF,
  SCM_LANE_WIDTH,
} from "./scm-history";

describe("parseHistoryRef", () => {
  test("splits head, tags, and remotes", () => {
    expect(parseHistoryRef("HEAD -> main")).toEqual({ name: "main", kind: "head" });
    expect(parseHistoryRef("tag: v1")).toEqual({ name: "v1", kind: "tag" });
    expect(parseHistoryRef("origin/main")).toEqual({ name: "origin/main", kind: "remote" });
    expect(parseHistoryRef("feature")).toEqual({ name: "feature", kind: "branch" });
  });
});

describe("historyRefBadges", () => {
  test("shows the first ref and collapses the rest by kind", () => {
    const badges = historyRefBadges({
      hash: "a",
      parents: [],
      subject: "",
      author: "",
      committedAt: "",
      refs: ["HEAD -> main", "origin/main", "origin/release", "tag: v1"],
    });
    expect(badges.map((badge) => [badge.kind, badge.name, badge.count])).toEqual([
      ["head", "main", 0],
      ["remote", "", 2],
      ["tag", "", 0],
    ]);
    expect(badges[1].title).toBe("origin/main\norigin/release");
  });
});

describe("layoutHistory", () => {
  test("keeps a straight line on the first lane", () => {
    const rows = layoutHistory([
      {
        hash: "b",
        parents: ["a"],
        subject: "second",
        author: "Ada",
        committedAt: "2026-01-02",
        refs: ["HEAD -> main"],
      },
      {
        hash: "a",
        parents: [],
        subject: "first",
        author: "Ada",
        committedAt: "2026-01-01",
        refs: [],
      },
    ]);
    expect(rows.map((row) => row.column)).toEqual([0, 0]);
    expect(rows[0].incoming).toBe(false);
    expect(rows[0].links).toEqual([{ from: 0, to: 0, color: rows[0].links[0].color }]);
    expect(rows[1].incoming).toBe(true);
    expect(rows[1].links).toEqual([]);
    expect(isHistoryHead(rows[0].commit)).toBe(true);
    expect(historyRefs(rows[0].commit)[0]).toEqual({ name: "main", kind: "head" });
  });

  test("opens a second lane for a merge parent", () => {
    const rows = layoutHistory([
      {
        hash: "m",
        parents: ["a", "b"],
        subject: "merge",
        author: "Ada",
        committedAt: "2026-01-03",
        refs: [],
      },
      {
        hash: "b",
        parents: [],
        subject: "side",
        author: "Ada",
        committedAt: "2026-01-02",
        refs: [],
      },
      {
        hash: "a",
        parents: [],
        subject: "main",
        author: "Ada",
        committedAt: "2026-01-01",
        refs: [],
      },
    ]);
    expect(rows[0].column).toBe(0);
    expect(rows[0].links.map((link) => link.to).sort()).toEqual([0, 1]);
    expect(rows[1].column).not.toBe(rows[2].column);
  });
});

describe("layoutHistoryGraph", () => {
  test("draws a continuous stem and a merge curve", () => {
    const rows = layoutHistoryGraph([
      {
        hash: "m",
        parents: ["a", "b"],
        subject: "merge",
        author: "Ada",
        committedAt: "2026-01-03",
        refs: ["HEAD -> main"],
      },
      {
        hash: "b",
        parents: ["a"],
        subject: "side",
        author: "Ada",
        committedAt: "2026-01-02",
        refs: [],
      },
      {
        hash: "a",
        parents: [],
        subject: "main",
        author: "Ada",
        committedAt: "2026-01-01",
        refs: [],
      },
    ]);
    expect(historyCircleIndex(rows[0])).toBe(0);
    expect(rows[0].output.map((lane) => lane.id)).toEqual(["a", "b"]);
    expect(rows[1].input.map((lane) => lane.id)).toEqual(["a", "b"]);
    const graph = historyGraph(rows[0]);
    expect(graph.paths.some((path) => path.d.includes("A 11 11"))).toBe(true);
    expect(graph.paths.some((path) => path.d.includes("V 22"))).toBe(true);
    // HEAD 优先于 merge 圆点样式：外 7 / 内 2
    expect(graph.dots[0].hole).toBe(false);
    expect(graph.dots[0].r).toBe(7);
    expect(graph.dots.at(-1)?.r).toBe(2);
    expect(graph.dots.at(-1)?.hole).toBe(true);
    expect(historyGraphWidth(rows)).toBeGreaterThan(11);
    expect(historyRowGraphWidth(rows[0])).toBe(SCM_LANE_WIDTH * 3);
    const mergeOnly = historyGraph({ ...rows[0], kind: "node" });
    expect(mergeOnly.dots.map((dot) => dot.r)).toEqual([6, 3]);
    // HEAD 泳道用 historyItemRefColor（蓝），侧枝走轮转色
    expect(historyCircleColor(rows[0])).toBe(SCM_COLOR_REF);
    expect(rows[0].output.map((lane) => lane.color)).toEqual([SCM_COLOR_REF, 0]);
    const mapped = layoutHistoryGraph(rows.map((row) => row.commit), historyColorMap("main", "origin/main"));
    expect(historyCircleColor(mapped[0])).toBe(SCM_COLOR_REF);
  });
});
