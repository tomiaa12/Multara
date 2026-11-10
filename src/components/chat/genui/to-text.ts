// 界面块转成纯文字：给只认文字的地方用（IM 渠道、复制为文字）。
// 前端版本；后端 IM 出站目前只剥掉界面块，以后要在 Rust 里实现同样的规则。
import type { CardProps, ChartProps, FormProps, StatsProps, TableProps } from "./schema";

const MAX_TEXT_ROWS = 20;

function cell(value: unknown) {
  return value === null || value === undefined ? "" : String(value).replace(/\s+/g, " ").trim();
}

export function cardToText(props: CardProps) {
  const lines = props.fields.map((field) => `${field.label}: ${field.value}`);
  if (props.body) lines.push(props.body);
  if (props.actions.length) lines.push(`[${props.actions.map((action) => action.label).join(" / ")}]`);
  return lines.join("\n");
}

export function statsToText(props: StatsProps) {
  return props.items
    .map((item) => {
      const delta = item.delta === null ? "" : ` (${item.delta > 0 ? "+" : ""}${item.delta})`;
      return `${item.label}: ${item.value}${item.unit}${delta}`;
    })
    .join("\n");
}

export function tableToText(props: TableProps) {
  const head = props.columns.map((column) => cell(column.label)).join(" | ");
  const rows = props.rows
    .slice(0, MAX_TEXT_ROWS)
    .map((row) => props.columns.map((column) => cell(row[column.key])).join(" | "));
  const more = props.rows.length > MAX_TEXT_ROWS ? [`… (${props.rows.length})`] : [];
  return [head, ...rows, ...more].join("\n");
}

/** 图表转成每个系列的数据摘要：点数、最小、最大、最后一个值 */
export function chartToText(props: ChartProps) {
  const lines = props.series.map((series) => {
    const values = props.data.map((row) => Number(row[series.key]) || 0);
    const last = props.data[props.data.length - 1];
    const lastLabel = last ? cell(last[props.xKey]) : "";
    return `${series.label}: n=${values.length}, min=${Math.min(...values)}${props.unit}, max=${Math.max(...values)}${props.unit}, ${lastLabel}=${values[values.length - 1] ?? 0}${props.unit}`;
  });
  return lines.join("\n");
}

export function formToText(props: FormProps) {
  return props.fields
    .map((field) => {
      const options = field.options.length ? ` (${field.options.map((option) => option.label).join(" / ")})` : "";
      return `${field.label}${field.required ? "*" : ""}${options}`;
    })
    .join("\n");
}
