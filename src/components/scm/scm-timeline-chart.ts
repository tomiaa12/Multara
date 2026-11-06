/**
 * Visual History 画布 — 对齐 GitLens `timelineRenderer` / `timelineData`（自研 canvas，无第三方图表库）。
 * 关键差异：气泡半径上限 0.7×行高；magnitude 用 anchor 归一化，避免全是 weight=1 时大圆不明显、或相对 max 被压成点。
 */

export type TimelinePeriod = "7|D" | "1|M" | "3|M" | "6|M" | "1|Y" | "2|Y" | "all";
export type TimelineSliceBy = "author" | "branch";

export interface TimelineCommit {
  hash: string;
  subject: string;
  author: string;
  at: number;
  weight: number;
  refs: string[];
}

export interface TimelineSlice {
  name: string;
  colorIndex: number;
  initials: string;
}

export interface TimelineRow {
  commitIndex: number;
  sliceIndex: number;
}

export interface TimelineViewModel {
  commits: TimelineCommit[];
  slices: TimelineSlice[];
  rows: TimelineRow[];
  oldest: number;
  newest: number;
  maxWeight: number;
  /** 对齐 GitLens p99，用于半径归一化。 */
  p99Weight: number;
}

export interface TimelineLayout {
  width: number;
  height: number;
  railWidth: number;
  chartLeft: number;
  chartRight: number;
  swimlaneTop: number;
  swimlaneBottom: number;
  rowHeight: number;
  virtualSwimlaneHeight: number;
  axisTop: number;
  volumeTop: number;
  volumeBottom: number;
  dataInsetX: number;
}

export interface TimelineTheme {
  background: string;
  zebra: string;
  axis: string;
  axisLabel: string;
  grid: string;
  bubbleStroke: string;
  volumeAdd: string;
  volumeDel: string;
  palette: readonly string[];
}

export interface AxisTick {
  at: number;
  x: number;
  label: string;
}

export interface BubbleHit {
  commitIndex: number;
  sliceIndex: number;
  x: number;
  y: number;
  r: number;
}

/** 对齐 GitLens TimelineDrawState 的悬停字段：由宿主 rAF 驱动 intensity。 */
export interface TimelineHoverState {
  /** 当前悬停气泡；volume scrub 时也指向对应提交。 */
  hit: BubbleHit | null;
  /** 0..1 线性进度，绘制前再 easeOutCubic。 */
  intensity: number;
  outgoing: BubbleHit | null;
  outgoingIntensity: number;
  /** 底部体积条悬停：提交下标，驱动竖向 spotlight。 */
  volumeCommitIndex: number | null;
}

const DAY_MS = 86_400_000;
const HOUR_MS = 3_600_000;
const MIN_ROW = 48;
const MAX_ROW = 72;
const RAIL_MIN = 40;
const RAIL_MAX = 160;
const AXIS_H = 22;
const VOLUME_H = 36;
const HEADER = 4;
/** 对齐 GitLens magnitudeAnchorLines=500：稀疏数据时避免全员顶到 max。文件数用更小 anchor。 */
const WEIGHT_ANCHOR = 12;
/** 对齐 GitLens `_hoverAnimDurationMs`。 */
export const HOVER_ANIM_MS = 140;
/** 对齐 GitLens hover scale 1.45x。 */
const HOVER_SCALE = 1.45;

export const PERIOD_OPTIONS: Array<{ id: TimelinePeriod; labelKey: string }> = [
  { id: "7|D", labelKey: "scm.inspect.period1w" },
  { id: "1|M", labelKey: "scm.inspect.period1m" },
  { id: "3|M", labelKey: "scm.inspect.period3m" },
  { id: "6|M", labelKey: "scm.inspect.period6m" },
  { id: "1|Y", labelKey: "scm.inspect.period1y" },
  { id: "2|Y", labelKey: "scm.inspect.period2y" },
  { id: "all", labelKey: "scm.inspect.periodAll" },
];

export const SLICE_PALETTE = [
  "#3D7EFF", "#FF9F40", "#1AB394", "#E91E63", "#9C27B0",
  "#8B5A2B", "#FF6B9D", "#7B7B7B", "#FFC107", "#26A69A",
] as const;

export function periodToMs(period: TimelinePeriod): number | null {
  if (period === "all") return null;
  const [n, unit] = period.split("|");
  const count = Number(n) || 1;
  if (unit === "D") return count * DAY_MS;
  if (unit === "M") return count * 30.4375 * DAY_MS;
  if (unit === "Y") return count * 365.25 * DAY_MS;
  return null;
}

export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return "?";
  // 中文名取前两字；英文取首字母。
  if (parts.length === 1) {
    const one = parts[0];
    if (/[\u4e00-\u9fff]/.test(one)) return one.slice(0, 2);
    return one.slice(0, 2).toUpperCase();
  }
  return `${parts[0][0] ?? ""}${parts[1][0] ?? ""}`.toUpperCase();
}

export function branchesFromRefs(refs: string[], fallback: string): string[] {
  const out: string[] = [];
  for (const raw of refs ?? []) {
    let name = raw.trim();
    if (!name || name === "HEAD") continue;
    if (name.startsWith("tag:") || name.startsWith("refs/tags/")) continue;
    name = name.replace(/^refs\/heads\//, "").replace(/^origin\//, "");
    if (name && !out.includes(name)) out.push(name);
  }
  return out.length ? out : [fallback || "HEAD"];
}

/** 窗口锚定在数据集最新提交（而非 Date.now），避免旧文件在「近 3 个月」被滤空却仍残留错误轴。 */
export function filterByPeriod(commits: TimelineCommit[], period: TimelinePeriod): TimelineCommit[] {
  const ms = periodToMs(period);
  if (ms == null || !commits.length) return commits;
  let newest = 0;
  for (const item of commits) if (item.at > newest) newest = item.at;
  const oldest = newest - ms;
  return commits.filter((item) => item.at >= oldest);
}

function percentile(sorted: number[], p: number): number {
  if (!sorted.length) return 1;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.ceil(p * sorted.length) - 1));
  return Math.max(1, sorted[idx]);
}

export function buildTimelineViewModel(
  commits: TimelineCommit[],
  sliceBy: TimelineSliceBy,
  defaultBranch: string,
): TimelineViewModel {
  const slices: TimelineSlice[] = [];
  const indexByName = new Map<string, number>();
  const ensure = (name: string) => {
    const key = name || "?";
    const hit = indexByName.get(key);
    if (hit != null) return hit;
    const index = slices.length;
    indexByName.set(key, index);
    slices.push({ name: key, colorIndex: index % SLICE_PALETTE.length, initials: initialsOf(key) });
    return index;
  };

  const rows: TimelineRow[] = [];
  for (let commitIndex = 0; commitIndex < commits.length; commitIndex += 1) {
    const commit = commits[commitIndex];
    const names = sliceBy === "branch"
      ? branchesFromRefs(commit.refs, defaultBranch)
      : [commit.author || "?"];
    for (const name of names) {
      rows.push({ commitIndex, sliceIndex: ensure(name) });
    }
  }

  let oldest = Number.POSITIVE_INFINITY;
  let newest = 0;
  let maxWeight = 1;
  const weights: number[] = [];
  for (const commit of commits) {
    if (commit.at < oldest) oldest = commit.at;
    if (commit.at > newest) newest = commit.at;
    const w = Math.max(1, commit.weight || 1);
    if (w > maxWeight) maxWeight = w;
    weights.push(w);
  }
  if (!Number.isFinite(oldest) || !commits.length) {
    const now = Date.now();
    oldest = now - DAY_MS;
    newest = now;
  }
  // 左右各留 4% 域边距，避免首尾气泡贴边；过短跨度抬到至少 1 天，轴可读。
  const span = Math.max(DAY_MS, newest - oldest);
  const pad = span * 0.04;
  oldest -= pad;
  newest = oldest + span + pad * 2;

  weights.sort((a, b) => a - b);
  return {
    commits,
    slices,
    rows,
    oldest,
    newest,
    maxWeight,
    p99Weight: percentile(weights, 0.99),
  };
}

/** 左侧轨宽：作者模式固定头像列；分支模式按最长名字自适应（对齐 GitLens pickRailColumnWidth）。 */
export function pickRailWidth(sliceBy: TimelineSliceBy, slices: TimelineSlice[], chartWidth: number): number {
  if (sliceBy !== "branch" || !slices.length) return RAIL_MIN;
  let longest = 0;
  for (const slice of slices) longest = Math.max(longest, slice.name.length);
  // 与文件名面包屑同量级：按字符估宽，受画布宽度上限约束，保证左对齐可读。
  const estimated = 28 + Math.min(longest, 28) * 7.2;
  const maxAllowed = Math.min(RAIL_MAX, Math.max(RAIL_MIN, Math.floor(chartWidth * 0.42)));
  return Math.round(Math.min(maxAllowed, Math.max(RAIL_MIN, estimated)));
}

export function computeTimelineLayout(
  width: number,
  height: number,
  sliceCount: number,
  railWidth = RAIL_MIN,
): TimelineLayout {
  const chartLeft = railWidth;
  const chartRight = Math.max(chartLeft + 80, width - 8);
  // 对齐 GitLens：体积条贴画布底，X 轴刻度叠在体积条上方；矮画布隐藏体积条保气泡区。
  const showVolume = height >= 200;
  const volumeBottom = height;
  const volumeTop = showVolume ? Math.max(HEADER + MIN_ROW, volumeBottom - VOLUME_H) : volumeBottom;
  const axisTop = Math.max(HEADER + MIN_ROW / 2, volumeTop - AXIS_H);
  const swimlaneTop = HEADER;
  const swimlaneBottom = Math.max(swimlaneTop + MIN_ROW, axisTop);
  const available = Math.max(MIN_ROW, swimlaneBottom - swimlaneTop);
  const fit = sliceCount > 0 ? Math.floor(available / sliceCount) : MAX_ROW;
  const rowHeight = Math.max(MIN_ROW, Math.min(MAX_ROW, fit || MAX_ROW));
  const cap = radiusCap(rowHeight);
  return {
    width,
    height,
    railWidth,
    chartLeft,
    chartRight,
    swimlaneTop,
    swimlaneBottom,
    rowHeight,
    virtualSwimlaneHeight: Math.max(rowHeight, sliceCount * rowHeight),
    axisTop,
    volumeTop,
    volumeBottom,
    dataInsetX: Math.round(cap + 6),
  };
}

export function tsToX(at: number, oldest: number, newest: number, layout: TimelineLayout): number {
  const span = Math.max(1, newest - oldest);
  const left = layout.chartLeft + layout.dataInsetX;
  const right = layout.chartRight - layout.dataInsetX;
  return left + ((at - oldest) / span) * Math.max(1, right - left);
}

/** 对齐 GitLens clampRadiusToRow：最大气泡 ≈ 0.7 行高。 */
export function radiusCap(rowHeight: number): number {
  return Math.max(6, rowHeight * 0.7);
}

export function bubbleMagnitude(weight: number, p99: number, maxWeight: number): number {
  const total = Math.max(0, weight);
  if (total <= 0 || maxWeight <= 0) return 0;
  const denom = Math.max(p99, WEIGHT_ANCHOR);
  return Math.min(1, Math.max(0, Math.log1p(total) / Math.log1p(denom)));
}

export function bubbleRadius(weight: number, p99: number, maxWeight: number, rowHeight: number): number {
  const cap = radiusCap(rowHeight);
  // 对齐 GitLens：min 取 1.5 量级，magnitude 直接映射半径（不再二次 sqrt），大提交更醒目。
  const minR = Math.min(1.5, cap);
  const t = bubbleMagnitude(weight, p99, maxWeight);
  // 至少给可见底盘，避免 weight=1 / 权重未回填时「看不见点」。
  const r = minR + (cap - minR) * t;
  return Math.max(4, r);
}

export function sliceCenterY(sliceIndex: number, layout: TimelineLayout, scrollY = 0): number {
  return layout.swimlaneTop + sliceIndex * layout.rowHeight + layout.rowHeight / 2 - scrollY;
}

export function buildAxisTicks(
  oldest: number,
  newest: number,
  layout: TimelineLayout,
): AxisTick[] {
  const span = newest - oldest;
  let step = DAY_MS;
  if (span > 2 * 365 * DAY_MS) step = 90 * DAY_MS;
  else if (span > 180 * DAY_MS) step = 30 * DAY_MS;
  else if (span > 45 * DAY_MS) step = 7 * DAY_MS;
  else if (span > 10 * DAY_MS) step = DAY_MS;
  else if (span > 2 * DAY_MS) step = 12 * HOUR_MS;
  else step = Math.max(HOUR_MS, Math.floor(span / 4));

  const ticks: AxisTick[] = [];
  const push = (at: number) => {
    const x = Math.round(tsToX(at, oldest, newest, layout));
    if (x < layout.chartLeft || x > layout.chartRight) return;
    if (ticks.some((item) => Math.abs(item.x - x) < 52)) return;
    ticks.push({ at, x, label: formatTick(at, step) });
  };
  push(oldest);
  let cursor = Math.ceil(oldest / step) * step;
  while (cursor < newest) {
    push(cursor);
    cursor += step;
  }
  push(newest);
  return ticks;
}

function formatTick(at: number, step: number): string {
  const date = new Date(at);
  const mo = date.getMonth() + 1;
  const day = date.getDate();
  if (step >= 90 * DAY_MS) {
    return `${date.getFullYear()}-${String(mo).padStart(2, "0")}`;
  }
  if (step >= DAY_MS) {
    return `${mo}/${day}`;
  }
  return `${date.getHours()}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export function easeOutCubic(t: number): number {
  const clamped = Math.max(0, Math.min(1, t));
  return 1 - (1 - clamped) ** 3;
}

function locateHit(
  vm: TimelineViewModel,
  hit: BubbleHit,
  layout: TimelineLayout,
  scrollY: number,
): { cx: number; cy: number; baseR: number; color: string } | null {
  const commit = vm.commits[hit.commitIndex];
  const slice = vm.slices[hit.sliceIndex];
  if (!commit || !slice) return null;
  const cx = tsToX(commit.at, vm.oldest, vm.newest, layout);
  const cy = sliceCenterY(hit.sliceIndex, layout, scrollY);
  if (!Number.isFinite(cx)) return null;
  const baseR = bubbleRadius(commit.weight, vm.p99Weight, vm.maxWeight, layout.rowHeight);
  return {
    cx,
    cy,
    baseR,
    color: SLICE_PALETTE[slice.colorIndex % SLICE_PALETTE.length],
  };
}

/** 对齐 GitLens drawHoverHighlight：径向光晕 + 1.45x 放大 + 双色描边。 */
function drawHoverHighlight(
  ctx: CanvasRenderingContext2D,
  located: { cx: number; cy: number; baseR: number; color: string },
  theme: TimelineTheme,
  intensity: number,
  layout: TimelineLayout,
): void {
  if (intensity <= 0) return;
  const { cx, cy, baseR, color } = located;
  const r = baseR * (1 + (HOVER_SCALE - 1) * intensity);
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, layout.width, layout.volumeTop);
  ctx.clip();

  const haloR = r * 2.6;
  const grad = ctx.createRadialGradient(cx, cy, r * 0.6, cx, cy, haloR);
  grad.addColorStop(0, color);
  grad.addColorStop(1, "rgba(0,0,0,0)");
  ctx.globalAlpha = 0.45 * intensity;
  ctx.fillStyle = grad;
  ctx.beginPath();
  ctx.arc(cx, cy, haloR, 0, Math.PI * 2);
  ctx.fill();
  ctx.globalAlpha = 1;

  ctx.shadowColor = color;
  ctx.shadowBlur = 14 * intensity;
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.fill();
  ctx.shadowBlur = 0;
  ctx.shadowColor = "transparent";

  ctx.lineWidth = 1.5;
  ctx.strokeStyle = theme.background;
  ctx.beginPath();
  ctx.arc(cx, cy, r + 0.75, 0, Math.PI * 2);
  ctx.stroke();
  ctx.lineWidth = 1;
  ctx.strokeStyle = color;
  ctx.beginPath();
  ctx.arc(cx, cy, r + 2, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();
}

function drawFocusLine(
  ctx: CanvasRenderingContext2D,
  x: number,
  top: number,
  bottom: number,
  color: string,
  alpha: number,
): void {
  if (alpha <= 0) return;
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.strokeStyle = color;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(Math.round(x) + 0.5, top);
  ctx.lineTo(Math.round(x) + 0.5, bottom);
  ctx.stroke();
  ctx.restore();
}

export function drawTimeline(
  ctx: CanvasRenderingContext2D,
  vm: TimelineViewModel,
  layout: TimelineLayout,
  theme: TimelineTheme,
  hover: TimelineHoverState | null,
  scrollY = 0,
): void {
  const { width, height } = layout;
  ctx.clearRect(0, 0, width, height);
  ctx.fillStyle = theme.background;
  ctx.fillRect(0, 0, width, height);

  const oldest = vm.oldest;
  const newest = vm.newest;
  const intensity = easeOutCubic(hover?.intensity ?? 0);
  const outgoingIntensity = easeOutCubic(hover?.outgoingIntensity ?? 0);
  const volumeIdx = hover?.volumeCommitIndex ?? null;

  // zebra + lane guides
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, layout.swimlaneTop, width, layout.swimlaneBottom - layout.swimlaneTop);
  ctx.clip();
  for (let i = 0; i < vm.slices.length; i += 1) {
    const y = layout.swimlaneTop + i * layout.rowHeight - scrollY;
    if (y + layout.rowHeight < layout.swimlaneTop || y > layout.swimlaneBottom) continue;
    if (i % 2 === 1) {
      ctx.fillStyle = theme.zebra;
      ctx.fillRect(layout.chartLeft, y, Math.max(0, layout.chartRight - layout.chartLeft), layout.rowHeight);
    }
    ctx.fillStyle = theme.grid;
    ctx.globalAlpha = 0.35;
    ctx.fillRect(
      layout.chartLeft,
      Math.round(y + layout.rowHeight / 2),
      Math.max(0, layout.chartRight - layout.chartLeft),
      1,
    );
    ctx.globalAlpha = 1;
  }

  // 静态气泡：悬停放大只在 overlay 画，底层保持原半径（对齐 GitLens）。
  for (const row of vm.rows) {
    const commit = vm.commits[row.commitIndex];
    const slice = vm.slices[row.sliceIndex];
    if (!commit || !slice) continue;
    const x = tsToX(commit.at, oldest, newest, layout);
    if (!Number.isFinite(x)) continue;
    const y = sliceCenterY(row.sliceIndex, layout, scrollY);
    const r = bubbleRadius(commit.weight, vm.p99Weight, vm.maxWeight, layout.rowHeight);
    ctx.beginPath();
    ctx.fillStyle = theme.palette[slice.colorIndex % theme.palette.length];
    ctx.globalAlpha = 0.7;
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.strokeStyle = theme.bubbleStroke;
    ctx.lineWidth = 1;
    ctx.stroke();
  }
  ctx.restore();

  // volume strip
  const volH = layout.volumeBottom - layout.volumeTop;
  if (volH > 0) {
    const bins = 48;
    const innerLeft = layout.chartLeft + layout.dataInsetX;
    const innerRight = layout.chartRight - layout.dataInsetX;
    const binW = Math.max(1, (innerRight - innerLeft) / bins);
    const counts = new Array(bins).fill(0);
    for (const commit of vm.commits) {
      const idx = Math.min(bins - 1, Math.floor(((commit.at - oldest) / Math.max(1, newest - oldest)) * bins));
      counts[idx] += Math.max(1, commit.weight);
    }
    const maxBin = Math.max(1, ...counts);
    let volumeX: number | null = null;
    if (volumeIdx != null) {
      const commit = vm.commits[volumeIdx];
      if (commit) volumeX = tsToX(commit.at, oldest, newest, layout);
    }
    for (let i = 0; i < bins; i += 1) {
      if (counts[i] <= 0) continue;
      const h = Math.sqrt(counts[i] / maxBin) * volH;
      const x = innerLeft + i * binW;
      const mid = x + binW / 2;
      const focused = volumeX != null && Math.abs(mid - volumeX) <= binW;
      ctx.globalAlpha = volumeX == null ? 0.55 : focused ? 0.9 : 0.18;
      ctx.fillStyle = theme.volumeAdd;
      ctx.fillRect(x + 0.5, layout.volumeBottom - h, Math.max(1, binW - 1), h * 0.65);
      ctx.fillStyle = theme.volumeDel;
      ctx.fillRect(x + 0.5, layout.volumeBottom - h * 0.35, Math.max(1, binW - 1), h * 0.35);
      ctx.globalAlpha = 1;
    }
  }

  // X axis
  ctx.strokeStyle = theme.axis;
  ctx.beginPath();
  ctx.moveTo(layout.chartLeft, layout.volumeTop + 0.5);
  ctx.lineTo(layout.chartRight, layout.volumeTop + 0.5);
  ctx.stroke();
  ctx.font = "10px var(--font-family, sans-serif)";
  ctx.textAlign = "center";
  ctx.textBaseline = "bottom";
  for (const tick of buildAxisTicks(oldest, newest, layout)) {
    ctx.fillStyle = theme.axis;
    ctx.fillRect(tick.x, layout.volumeTop - 3, 1, 3);
    ctx.fillStyle = theme.axisLabel;
    ctx.fillText(tick.label, tick.x, layout.volumeTop - 5);
  }

  // —— overlay：对齐 GitLens drawOverlay ——
  // volume spotlight：只留一列清晰，其余泳道压暗
  if (volumeIdx != null) {
    const commit = vm.commits[volumeIdx];
    if (commit) {
      const vx = tsToX(commit.at, oldest, newest, layout);
      if (Number.isFinite(vx)) {
        const half = radiusCap(layout.rowHeight) + 6;
        const left = Math.max(layout.chartLeft, vx - half);
        const right = Math.min(layout.chartRight, vx + half);
        ctx.fillStyle = theme.background;
        ctx.globalAlpha = 0.55;
        if (left > layout.chartLeft) {
          ctx.fillRect(layout.chartLeft, 0, left - layout.chartLeft, layout.volumeTop);
        }
        if (right < layout.chartRight) {
          ctx.fillRect(right, 0, layout.chartRight - right, layout.volumeTop);
        }
        ctx.globalAlpha = 1;
        drawFocusLine(ctx, vx, layout.swimlaneTop, layout.volumeBottom, theme.grid, 0.55);
      }
    }
  }

  // 气泡悬停 sibling dim
  const dimIntensity = intensity + outgoingIntensity * 0.6;
  if (dimIntensity > 0.01 && volumeIdx == null) {
    ctx.fillStyle = theme.background;
    ctx.globalAlpha = 0.35 * dimIntensity;
    ctx.fillRect(0, 0, width, layout.volumeTop);
    ctx.globalAlpha = 1;
  }

  // 气泡悬停竖线（从顶到体积条底）
  if (hover?.hit && intensity > 0.01) {
    const located = locateHit(vm, hover.hit, layout, scrollY);
    if (located) {
      drawFocusLine(ctx, located.cx, HEADER, layout.volumeBottom, theme.grid, 0.3 + 0.4 * intensity);
    }
  }

  // outgoing → incoming 交叉淡入淡出
  if (hover?.outgoing && outgoingIntensity > 0.01) {
    const same =
      hover.hit
      && hover.outgoing.commitIndex === hover.hit.commitIndex
      && hover.outgoing.sliceIndex === hover.hit.sliceIndex;
    if (!same) {
      const located = locateHit(vm, hover.outgoing, layout, scrollY);
      if (located) drawHoverHighlight(ctx, located, theme, outgoingIntensity, layout);
    }
  }
  if (hover?.hit && intensity > 0.01) {
    const located = locateHit(vm, hover.hit, layout, scrollY);
    if (located) drawHoverHighlight(ctx, located, theme, intensity, layout);
  }
}

export function hitTestBubble(
  x: number,
  y: number,
  vm: TimelineViewModel,
  layout: TimelineLayout,
  scrollY = 0,
): BubbleHit | null {
  if (y < layout.swimlaneTop || y > layout.swimlaneBottom) return null;
  if (x < layout.chartLeft || x > layout.chartRight) return null;
  let best: BubbleHit | null = null;
  let bestDist = Infinity;
  for (const row of vm.rows) {
    const commit = vm.commits[row.commitIndex];
    if (!commit) continue;
    const cx = tsToX(commit.at, vm.oldest, vm.newest, layout);
    const cy = sliceCenterY(row.sliceIndex, layout, scrollY);
    const r = bubbleRadius(commit.weight, vm.p99Weight, vm.maxWeight, layout.rowHeight) + 4;
    const dist = Math.hypot(x - cx, y - cy);
    if (dist <= r && dist < bestDist) {
      bestDist = dist;
      best = { commitIndex: row.commitIndex, sliceIndex: row.sliceIndex, x: cx, y: cy, r };
    }
  }
  return best;
}

/** 对齐 GitLens findNearestVolumeBar：指针在体积条内时锁定最近提交，驱动气泡联动。 */
export function findNearestVolumeCommit(
  x: number,
  y: number,
  vm: TimelineViewModel,
  layout: TimelineLayout,
): BubbleHit | null {
  if (y < layout.volumeTop || y > layout.volumeBottom) return null;
  if (x < layout.chartLeft || x > layout.chartRight) return null;
  if (!vm.commits.length) return null;

  let bestCommit = -1;
  let bestDx = Infinity;
  let bestWeight = -1;
  for (let i = 0; i < vm.commits.length; i += 1) {
    const commit = vm.commits[i];
    const cx = tsToX(commit.at, vm.oldest, vm.newest, layout);
    if (!Number.isFinite(cx) || cx < layout.chartLeft || cx > layout.chartRight) continue;
    const dx = Math.abs(x - cx);
    const w = Math.max(1, commit.weight);
    if (dx < bestDx || (dx === bestDx && w > bestWeight)) {
      bestDx = dx;
      bestWeight = w;
      bestCommit = i;
    }
  }
  if (bestCommit < 0) return null;

  // 取该提交在任意切片上的一条泳道，优先最大半径位置无关，取第一条 row。
  const row = vm.rows.find((item) => item.commitIndex === bestCommit);
  if (!row) return null;
  const commit = vm.commits[bestCommit];
  const cx = tsToX(commit.at, vm.oldest, vm.newest, layout);
  const cy = sliceCenterY(row.sliceIndex, layout, 0);
  const r = bubbleRadius(commit.weight, vm.p99Weight, vm.maxWeight, layout.rowHeight);
  return { commitIndex: bestCommit, sliceIndex: row.sliceIndex, x: cx, y: cy, r };
}

export function sameBubbleHit(a: BubbleHit | null, b: BubbleHit | null): boolean {
  if (a == null && b == null) return true;
  if (a == null || b == null) return false;
  return a.commitIndex === b.commitIndex && a.sliceIndex === b.sliceIndex;
}

export function readTimelineTheme(el: HTMLElement): TimelineTheme {
  const styles = getComputedStyle(el);
  return {
    background: styles.getPropertyValue("--color-surface").trim() || "#1e1e1e",
    zebra: "rgba(255,255,255,0.035)",
    axis: styles.getPropertyValue("--color-border").trim() || "#3c3c3c",
    axisLabel: styles.getPropertyValue("--color-text-secondary").trim() || "#999",
    grid: "rgba(255,255,255,0.1)",
    bubbleStroke: "rgba(255,255,255,0.28)",
    volumeAdd: "#3fb950",
    volumeDel: "#f85149",
    palette: SLICE_PALETTE,
  };
}
