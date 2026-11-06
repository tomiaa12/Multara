import { describe, expect, it } from "vitest";
import {
  PANE_HEADER,
  fitHeightsToTotal,
  layoutPaneHeights,
  paneMax,
  paneMin,
  resizeAroundSash,
  sashCursor,
  sashDisabled,
  sashTargetIndex,
  type PaneBox,
} from "./pane-layout";

function pane(partial: Partial<PaneBox> & Pick<PaneBox, "fill" | "expanded">): PaneBox {
  return { minBody: 120, preferred: 200, ...partial };
}

describe("layoutPaneHeights", () => {
  it("keeps changes on top and the graph at its default height on the bottom", () => {
    const heights = layoutPaneHeights(500, [
      pane({ fill: true, expanded: true }),
      pane({ fill: false, expanded: true, preferred: 200 }),
    ]);
    expect(heights).toEqual([300, 200]);
  });

  it("pins a collapsed graph header to the bottom", () => {
    const heights = layoutPaneHeights(500, [
      pane({ fill: true, expanded: true }),
      pane({ fill: false, expanded: false, preferred: 200 }),
    ]);
    expect(heights).toEqual([478, 22]);
  });

  it("puts a collapsed changes header on top and gives the rest to the graph", () => {
    const heights = layoutPaneHeights(500, [
      pane({ fill: true, expanded: false }),
      pane({ fill: false, expanded: true, preferred: 200 }),
    ]);
    expect(heights).toEqual([22, 478]);
  });

  it("stacks every collapsed header at the top", () => {
    const heights = layoutPaneHeights(400, [
      pane({ fill: false, expanded: false, minBody: 22, preferred: 80 }),
      pane({ fill: true, expanded: false, minBody: 120 }),
      pane({ fill: false, expanded: false, minBody: 22, preferred: 90 }),
    ]);
    expect(heights).toEqual([22, 22, 22]);
  });

  it("keeps preferred heights and scrolls when drawers no longer fit", () => {
    const heights = layoutPaneHeights(300, [
      pane({ fill: true, expanded: true, minBody: 120 }),
      pane({ fill: false, expanded: true, preferred: 200, minBody: 120 }),
    ]);
    // fill 到 min，非 fill 保留 preferred，外层滚动，避免压死后无法再拖。
    expect(heights).toEqual([142, 200]);
  });

  it("scrolls when many non-fill drawers prefer more than the viewport", () => {
    const heights = layoutPaneHeights(400, [
      pane({ fill: false, expanded: true, preferred: 180, minBody: 60 }),
      pane({ fill: false, expanded: true, preferred: 180, minBody: 60 }),
      pane({ fill: false, expanded: true, preferred: 180, minBody: 60 }),
    ]);
    expect(heights).toEqual([180, 180, 180]);
  });

  it("stacks collapsed drawers above and below a flexible explorer", () => {
    const heights = layoutPaneHeights(400, [
      pane({ fill: false, expanded: false, minBody: 22, preferred: 80 }),
      pane({ fill: true, expanded: true, minBody: 120 }),
      pane({ fill: false, expanded: false, minBody: 22, preferred: 90 }),
    ]);
    expect(heights).toEqual([22, 356, 22]);
  });
});

describe("sashTargetIndex / sashDisabled", () => {
  it("resizes a non-fill drawer even when the pane above is collapsed", () => {
    const panes = [
      pane({ fill: true, expanded: true }),
      pane({ fill: false, expanded: false, preferred: 200 }),
      pane({ fill: false, expanded: true, preferred: 220, minBody: 60 }),
    ];
    expect(sashTargetIndex(panes, 2)).toBe(2);
    expect(sashDisabled(panes, 2)).toBe(false);
  });

  it("disables the sash when the resize target itself is collapsed", () => {
    const panes = [
      pane({ fill: true, expanded: true }),
      pane({ fill: false, expanded: false, preferred: 200 }),
    ];
    expect(sashTargetIndex(panes, 1)).toBe(1);
    expect(sashDisabled(panes, 1)).toBe(true);
  });

  it("resizes the pane above when the sash belongs to a fill pane", () => {
    const panes = [
      pane({ fill: false, expanded: true, preferred: 180, minBody: 60 }),
      pane({ fill: true, expanded: true }),
    ];
    expect(sashTargetIndex(panes, 1)).toBe(0);
    expect(sashDisabled(panes, 1)).toBe(false);
  });
});

describe("resizeAroundSash", () => {
  it("grows the pane below by borrowing from above when dragging up", () => {
    // sash on index 2, delta < 0 → 上侧变矮、下侧变高，总和不变
    expect(resizeAroundSash([300, 200, 200], [142, 80, 80], 2, -50)).toEqual([300, 150, 250]);
  });

  it("borrows from farther panes when the nearest sibling hits its minimum", () => {
    // 上侧最近只能让出 20，剩余 30 继续向上借
    expect(resizeAroundSash([300, 100, 200], [142, 80, 80], 2, -50)).toEqual([270, 80, 250]);
  });

  it("grows the pane above by borrowing from below when dragging down", () => {
    expect(resizeAroundSash([300, 200, 200], [142, 80, 80], 2, 40)).toEqual([300, 240, 160]);
  });

  it("keeps a collapsed neighbor pinned to the header while resizing the expanded pane", () => {
    // [fill, 文件历史收起, 可视化历史展开] — 拖动时收起项高度始终是 header
    const mins = [142, PANE_HEADER, 80];
    const maxes = [Number.POSITIVE_INFINITY, PANE_HEADER, Number.POSITIVE_INFINITY];
    expect(resizeAroundSash([300, PANE_HEADER, 200], mins, 2, -50, maxes)).toEqual([250, PANE_HEADER, 250]);
    expect(resizeAroundSash([300, PANE_HEADER, 200], mins, 2, 40, maxes)).toEqual([340, PANE_HEADER, 160]);
  });
});

describe("paneMax", () => {
  it("pins collapsed panes to the header like VS Code maximumSize", () => {
    expect(paneMax(pane({ fill: false, expanded: false }))).toBe(PANE_HEADER);
    expect(paneMin(pane({ fill: false, expanded: false }))).toBe(PANE_HEADER);
    expect(paneMax(pane({ fill: false, expanded: true }))).toBe(Number.POSITIVE_INFINITY);
  });
});

describe("fitHeightsToTotal", () => {
  it("shrinks from the bottom until heights fit the viewport", () => {
    expect(fitHeightsToTotal([300, 200, 200], [142, 80, 80], 500)).toEqual([300, 120, 80]);
  });
});

describe("sashCursor", () => {
  it("points north when the lower pane cannot shrink", () => {
    expect(sashCursor(true, true, false)).toBe("n-resize");
  });

  it("points south when the lower pane cannot grow", () => {
    expect(sashCursor(true, false, true)).toBe("s-resize");
  });

  it("uses a two-way resize cursor away from the limits", () => {
    expect(sashCursor(false, false, false)).toBe("ns-resize");
  });
});
