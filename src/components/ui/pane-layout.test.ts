import { describe, expect, it } from "vitest";
import { layoutPaneHeights, sashCursor, type PaneBox } from "./pane-layout";

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

  it("does not let the graph shrink the changes pane below its minimum", () => {
    const heights = layoutPaneHeights(300, [
      pane({ fill: true, expanded: true, minBody: 120 }),
      pane({ fill: false, expanded: true, preferred: 200, minBody: 120 }),
    ]);
    expect(heights).toEqual([142, 158]);
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
