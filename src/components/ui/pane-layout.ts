/** Pane header. Keep in sync with `.ui-pane-header`. */
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

/** 对齐 VS Code Pane.maximumSize：收起时 max === header，拖动不会把空壳撑开。 */
export function paneMax(pane: PaneBox): number {
  return pane.expanded ? Number.POSITIVE_INFINITY : PANE_HEADER;
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
    return deficit;
  };

  if (fillIndexes.length === 0) {
    const preferredSum = panes.reduce((sum, pane) => {
      if (!pane.expanded) return sum + PANE_HEADER;
      return sum + Math.max(paneMin(pane), pane.preferred);
    }, 0);
    // preferred 总和高过视口时直接滚动，不要先压到 min（否则 sash 顶死后无法再拉高）。
    if (preferredSum > total) {
      return panes.map((pane) => {
        if (!pane.expanded) return PANE_HEADER;
        return Math.max(paneMin(pane), pane.preferred);
      });
    }
    const extra = total - fixed();
    if (extra > 0) {
      for (let index = heights.length - 1; index >= 0; index -= 1) {
        if (panes[index].expanded) {
          heights[index] += extra;
          break;
        }
      }
    }
    return heights.map((height) => Math.max(0, height));
  }

  const fillFloor = fillIndexes.reduce((sum, index) => sum + paneMin(panes[index]), 0);
  // 先按 preferred 估非 fill 占用；放不下就滚动，避免 shrink 成功后全部卡在 min。
  const preferredFixed = panes.reduce((sum, pane, index) => {
    if (heights[index] < 0) return sum;
    if (!pane.expanded) return sum + PANE_HEADER;
    if (pane.fill) return sum;
    return sum + Math.max(paneMin(pane), pane.preferred);
  }, 0);
  if (preferredFixed + fillFloor > total) {
    for (const index of fillIndexes) heights[index] = paneMin(panes[index]);
    for (let index = 0; index < heights.length; index += 1) {
      if (!panes[index].expanded || panes[index].fill) continue;
      heights[index] = Math.max(paneMin(panes[index]), panes[index].preferred);
    }
    return heights.map((height) => Math.max(0, height));
  }

  let rest = total - fixed();
  if (rest < fillFloor) {
    shrink(fillFloor - rest);
    rest = total - fixed();
  }

  const base = Math.floor(rest / fillIndexes.length);
  let leftover = rest - base * fillIndexes.length;
  for (const index of fillIndexes) {
    const next = base + (leftover > 0 ? 1 : 0);
    if (leftover > 0) leftover -= 1;
    heights[index] = next;
  }
  return heights.map((height) => Math.max(0, height));
}

/**
 * pane[index] 顶部分割条是否可用：只要分割线下方（或 fill 时上方目标）是展开的即可。
 * 上方折叠时仍应能拉当前抽屉（多 Inspect 抽屉场景）。
 */
export function sashTargetIndex(panes: readonly PaneBox[], index: number): number {
  if (index <= 0 || index >= panes.length) return -1;
  return panes[index].fill ? index - 1 : index;
}

export function sashDisabled(panes: readonly PaneBox[], index: number): boolean {
  const target = sashTargetIndex(panes, index);
  if (target < 0) return true;
  return !panes[target].expanded;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * 对齐 VS Code SplitView.resize：pane[index] 顶部分割条位于 index-1 | index 之间。
 * delta > 0（鼠标下移）→ 上侧变高、下侧变矮；总和不变，不会撑出滚动条。
 * 收起抽屉 max===min===header，跳过增减，紧贴当前展开抽屉一起移动。
 */
export function resizeAroundSash(
  startHeights: readonly number[],
  mins: readonly number[],
  sashPaneIndex: number,
  delta: number,
  maxes?: readonly number[],
): number[] {
  const heights = startHeights.map((height) => Math.max(0, height));
  const split = sashPaneIndex - 1;
  if (split < 0 || sashPaneIndex >= heights.length) return heights;
  if (mins.length !== heights.length) return heights;
  const caps = maxes && maxes.length === heights.length
    ? maxes
    : mins.map(() => Number.POSITIVE_INFINITY);

  const upIndexes: number[] = [];
  for (let index = split; index >= 0; index -= 1) upIndexes.push(index);
  const downIndexes: number[] = [];
  for (let index = sashPaneIndex; index < heights.length; index += 1) downIndexes.push(index);

  const minDeltaUp = upIndexes.reduce((sum, index) => sum + (mins[index] - startHeights[index]), 0);
  const maxDeltaUp = upIndexes.reduce((sum, index) => sum + (caps[index] - startHeights[index]), 0);
  const maxDeltaDown = downIndexes.reduce((sum, index) => sum + (startHeights[index] - mins[index]), 0);
  const minDeltaDown = downIndexes.reduce((sum, index) => sum + (startHeights[index] - caps[index]), 0);
  const clamped = clamp(delta, Math.max(minDeltaUp, minDeltaDown), Math.min(maxDeltaDown, maxDeltaUp));

  let deltaUp = clamped;
  for (const index of upIndexes) {
    const next = clamp(startHeights[index] + deltaUp, mins[index], caps[index]);
    deltaUp -= next - startHeights[index];
    heights[index] = next;
  }

  let deltaDown = clamped;
  for (const index of downIndexes) {
    const next = clamp(startHeights[index] - deltaDown, mins[index], caps[index]);
    deltaDown += next - startHeights[index];
    heights[index] = next;
  }

  return heights;
}

/** 拖动开始时若已溢出视口，先压回 total，避免带着滚动条再拖。 */
export function fitHeightsToTotal(
  heights: readonly number[],
  mins: readonly number[],
  total: number,
  maxes?: readonly number[],
): number[] {
  const caps = maxes && maxes.length === heights.length
    ? maxes
    : mins.map(() => Number.POSITIVE_INFINITY);
  const next = heights.map((height, index) => clamp(height, mins[index] ?? 0, caps[index] ?? Number.POSITIVE_INFINITY));
  let deficit = next.reduce((sum, height) => sum + height, 0) - total;
  if (deficit <= 0) return next;
  for (let index = next.length - 1; index >= 0 && deficit > 0; index -= 1) {
    const cut = Math.min(deficit, next[index] - (mins[index] ?? 0));
    if (cut > 0) {
      next[index] -= cut;
      deficit -= cut;
    }
  }
  return next;
}

/** Horizontal sash cursor (ns / n / s resize). */
export function sashCursor(resizingBelow: boolean, atMin: boolean, atMax: boolean): "ns-resize" | "n-resize" | "s-resize" {
  if (atMin === atMax) return "ns-resize";
  if (resizingBelow) return atMin ? "n-resize" : "s-resize";
  return atMin ? "s-resize" : "n-resize";
}
