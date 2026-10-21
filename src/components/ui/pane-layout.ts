/** VS Code Pane header. Keep in sync with `.ui-pane-header`. */
export const PANE_HEADER = 22;

export interface PaneBox {
  fill: boolean;
  expanded: boolean;
  /** Drag floor for the body while expanded. Collapsed size is always the header. */
  minBody: number;
  /** Wanted height including the header. Used for an expanded pane that is not flexible. */
  preferred: number;
}

export function paneMin(pane: PaneBox): number {
  return pane.expanded ? PANE_HEADER + Math.max(0, pane.minBody) : PANE_HEADER;
}

/**
 * Vertical pane heights, top to bottom.
 * A collapsed pane is only its header, so those headers stack from the top.
 * An expanded flexible pane (文件夹 / 更改) takes the leftover space.
 */
export function layoutPaneHeights(total: number, panes: PaneBox[]): number[] {
  if (total <= 0 || panes.length === 0) return panes.map(() => 0);

  const heights = panes.map((pane) => {
    if (!pane.expanded) return PANE_HEADER;
    if (pane.fill) return -1;
    return Math.max(paneMin(pane), pane.preferred);
  });

  const fillIndexes = heights.flatMap((height, index) => (height < 0 ? [index] : []));
  const fixed = () => heights.reduce((sum, height) => sum + (height < 0 ? 0 : height), 0);

  const shrink = (deficit: number) => {
    for (let index = heights.length - 1; index >= 0 && deficit > 0; index -= 1) {
      if (heights[index] < 0 || panes[index].fill || !panes[index].expanded) continue;
      const cut = Math.min(deficit, heights[index] - paneMin(panes[index]));
      if (cut > 0) {
        heights[index] -= cut;
        deficit -= cut;
      }
    }
  };

  if (fillIndexes.length === 0) {
    const extra = total - fixed();
    if (extra > 0) {
      for (let index = heights.length - 1; index >= 0; index -= 1) {
        if (panes[index].expanded) {
          heights[index] += extra;
          break;
        }
      }
    } else if (extra < 0) {
      shrink(-extra);
    }
    return heights.map((height) => Math.max(0, height));
  }

  const fillFloor = fillIndexes.reduce((sum, index) => sum + paneMin(panes[index]), 0);
  if (fixed() + fillFloor > total) shrink(fixed() + fillFloor - total);

  let rest = total - fixed();
  const base = Math.floor(rest / fillIndexes.length);
  let leftover = rest - base * fillIndexes.length;
  for (const index of fillIndexes) {
    const next = base + (leftover > 0 ? 1 : 0);
    if (leftover > 0) leftover -= 1;
    heights[index] = next;
  }
  return heights.map((height) => Math.max(0, height));
}

/** Horizontal sash cursor, matching VS Code's ns / n / s resize cursors. */
export function sashCursor(resizingBelow: boolean, atMin: boolean, atMax: boolean): "ns-resize" | "n-resize" | "s-resize" {
  if (atMin === atMax) return "ns-resize";
  if (resizingBelow) return atMin ? "n-resize" : "s-resize";
  return atMin ? "s-resize" : "n-resize";
}
