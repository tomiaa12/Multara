// 图表的纯几何计算：坐标轴刻度、折线点、柱子、饼图扇区。和渲染分开，方便单测。
import type { ChartProps } from "./schema";

export const SERIES_COLORS = [
  "var(--color-primary)",
  "var(--color-success)",
  "var(--color-warning)",
  "var(--color-purple)",
  "var(--color-info)",
  "var(--color-accent)",
  "var(--color-error)",
  "var(--color-text-secondary)",
];

export interface Box {
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
}

export const CHART_BOX: Box = { width: 640, height: 240, left: 48, right: 12, top: 12, bottom: 28 };

/** 1 / 2 / 5 × 10^n 的整齐刻度，含 0 */
export function niceTicks(min: number, max: number, count = 5): number[] {
  let lo = Math.min(0, min);
  let hi = Math.max(0, max);
  if (lo === hi) hi = lo + 1;
  const rough = (hi - lo) / Math.max(1, count);
  const power = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 5, 10].map((m) => m * power).find((value) => value >= rough) ?? 10 * power;
  lo = Math.floor(lo / step) * step;
  hi = Math.ceil(hi / step) * step;
  const ticks: number[] = [];
  for (let value = lo; value <= hi + step / 2; value += step) ticks.push(Number(value.toPrecision(12)));
  return ticks;
}

function plotWidth(box: Box) {
  return box.width - box.left - box.right;
}

function plotHeight(box: Box) {
  return box.height - box.top - box.bottom;
}

export function valueRange(props: ChartProps): { min: number; max: number } {
  let min = 0;
  let max = 0;
  for (const row of props.data) {
    if (props.stacked) {
      let positive = 0;
      let negative = 0;
      for (const series of props.series) {
        const value = Number(row[series.key]) || 0;
        if (value >= 0) positive += value;
        else negative += value;
      }
      max = Math.max(max, positive);
      min = Math.min(min, negative);
      continue;
    }
    for (const series of props.series) {
      const value = Number(row[series.key]) || 0;
      max = Math.max(max, value);
      min = Math.min(min, value);
    }
  }
  return { min, max };
}

export function yScale(ticks: number[], box: Box = CHART_BOX) {
  const lo = ticks[0];
  const hi = ticks[ticks.length - 1];
  const span = hi - lo || 1;
  return (value: number) => box.top + plotHeight(box) * (1 - (value - lo) / span);
}

/** 第 i 个类目的中心 x */
export function xCenter(index: number, count: number, box: Box = CHART_BOX) {
  const band = plotWidth(box) / Math.max(1, count);
  return box.left + band * (index + 0.5);
}

export function linePath(props: ChartProps, seriesKey: string, ticks: number[], box: Box = CHART_BOX) {
  const y = yScale(ticks, box);
  return props.data
    .map((row, index) => `${index ? "L" : "M"}${round(xCenter(index, props.data.length, box))},${round(y(Number(row[seriesKey]) || 0))}`)
    .join(" ");
}

export interface BarRect {
  series: number;
  row: number;
  x: number;
  y: number;
  width: number;
  height: number;
  value: number;
}

export function barRects(props: ChartProps, ticks: number[], box: Box = CHART_BOX): BarRect[] {
  const y = yScale(ticks, box);
  const zero = y(0);
  const count = props.data.length;
  const band = plotWidth(box) / Math.max(1, count);
  const inner = band * 0.72;
  const rects: BarRect[] = [];
  props.data.forEach((row, rowIndex) => {
    const start = box.left + band * rowIndex + (band - inner) / 2;
    let positive = 0;
    let negative = 0;
    props.series.forEach((series, seriesIndex) => {
      const value = Number(row[series.key]) || 0;
      if (props.stacked) {
        const base = value >= 0 ? positive : negative;
        const top = base + value;
        if (value >= 0) positive = top;
        else negative = top;
        const y1 = y(Math.max(base, top));
        const y2 = y(Math.min(base, top));
        rects.push({ series: seriesIndex, row: rowIndex, x: round(start), y: round(y1), width: round(inner), height: round(Math.max(0, y2 - y1)), value });
        return;
      }
      const width = inner / props.series.length;
      const top = value >= 0 ? y(value) : zero;
      rects.push({
        series: seriesIndex,
        row: rowIndex,
        x: round(start + width * seriesIndex),
        y: round(top),
        width: round(Math.max(1, width - 1)),
        height: round(Math.abs(y(value) - zero)),
        value,
      });
    });
  });
  return rects;
}

export interface PieSlice {
  row: number;
  label: string;
  value: number;
  share: number;
  path: string;
}

/** 负数和 0 不画扇区 */
export function pieSlices(props: ChartProps, cx = 120, cy = 120, radius = 100): PieSlice[] {
  const key = props.series[0]?.key;
  if (!key) return [];
  const values = props.data.map((row) => Math.max(0, Number(row[key]) || 0));
  const total = values.reduce((sum, value) => sum + value, 0);
  if (!total) return [];
  let angle = -Math.PI / 2;
  const slices: PieSlice[] = [];
  values.forEach((value, row) => {
    if (!value) return;
    const share = value / total;
    const next = angle + share * Math.PI * 2;
    const path = share >= 0.999999
      ? `M${cx - radius},${cy} a${radius},${radius} 0 1,0 ${radius * 2},0 a${radius},${radius} 0 1,0 ${-radius * 2},0`
      : [
        `M${cx},${cy}`,
        `L${round(cx + radius * Math.cos(angle))},${round(cy + radius * Math.sin(angle))}`,
        `A${radius},${radius} 0 ${share > 0.5 ? 1 : 0},1 ${round(cx + radius * Math.cos(next))},${round(cy + radius * Math.sin(next))}`,
        "Z",
      ].join(" ");
    slices.push({ row, label: String(props.data[row][props.xKey] ?? ""), value, share, path });
    angle = next;
  });
  return slices;
}

/** 类目太多时只标一部分 x 轴文字 */
export function labelStride(count: number, box: Box = CHART_BOX, minGap = 56) {
  const band = plotWidth(box) / Math.max(1, count);
  return Math.max(1, Math.ceil(minGap / band));
}

export function formatNumber(value: number) {
  const abs = Math.abs(value);
  if (abs >= 1e9) return `${Number((value / 1e9).toFixed(1))}B`;
  if (abs >= 1e6) return `${Number((value / 1e6).toFixed(1))}M`;
  if (abs >= 1e4) return `${Number((value / 1e3).toFixed(1))}k`;
  return Number(value.toFixed(2)).toLocaleString();
}

function round(value: number) {
  return Math.round(value * 10) / 10;
}
