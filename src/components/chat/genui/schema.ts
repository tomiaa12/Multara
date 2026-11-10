// 每种界面块的 props 校验与归一化。宽进严出：未知字段忽略，超上限截断并提示，
// 只有缺了关键字段才判失败（失败的块按代码块降级显示）。
import { isRecord } from "./block";

export interface Truncation {
  shown: number;
  total: number;
}

export type ValidateResult<P> =
  | { ok: true; props: P; truncated?: Truncation }
  | { ok: false; error: string };

export interface CardProps {
  fields: { label: string; value: string }[];
  body: string;
  /** openCanvas：点了打开已注册的 Canvas（不发消息）；Canvas 关闭时按钮隐藏 */
  actions: { id: string; label: string; style: "primary" | "default"; openCanvas?: string }[];
}

export interface StatsProps {
  items: { label: string; value: string; delta: number | null; unit: string }[];
}

export type ColumnFormat = "text" | "number" | "percent" | "date";
export type Cell = string | number | boolean | null;

export interface TableProps {
  columns: { key: string; label: string; align: "left" | "right"; format: ColumnFormat }[];
  rows: Record<string, Cell>[];
}

export interface ChartProps {
  chartType: "line" | "bar" | "pie";
  xKey: string;
  series: { key: string; label: string }[];
  data: Record<string, string | number>[];
  unit: string;
  stacked: boolean;
}

export type FormFieldKind =
  | "text" | "textarea" | "number" | "select" | "multiselect" | "date" | "slider" | "switch";

export interface FormField {
  name: string;
  label: string;
  kind: FormFieldKind;
  required: boolean;
  default: unknown;
  placeholder: string;
  options: { value: string; label: string }[];
  min: number | null;
  max: number | null;
  step: number | null;
}

export interface FormProps {
  fields: FormField[];
  submitLabel: string;
}

export const LIMITS = {
  cardFields: 40,
  cardActions: 4,
  statsItems: 8,
  tableColumns: 20,
  tableRows: 200,
  chartSeries: 8,
  chartPoints: 500,
  formFields: 40,
  formOptions: 100,
  text: 2000,
} as const;

const FORM_KINDS: FormFieldKind[] = ["text", "textarea", "number", "select", "multiselect", "date", "slider", "switch"];
const FORMATS: ColumnFormat[] = ["text", "number", "percent", "date"];

function str(value: unknown, max: number = LIMITS.text): string {
  if (typeof value === "string") return value.trim().slice(0, max);
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  if (typeof value === "boolean") return String(value);
  return "";
}

function num(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() && Number.isFinite(Number(value))) return Number(value);
  return null;
}

function list(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

/** 截到上限，记下原始条数 */
function cap<T>(items: T[], limit: number): { items: T[]; truncated?: Truncation } {
  if (items.length <= limit) return { items };
  return { items: items.slice(0, limit), truncated: { shown: limit, total: items.length } };
}

function mergeTruncation(...parts: (Truncation | undefined)[]): Truncation | undefined {
  // 多处截断时报行 / 点数那一处（最显眼的那个）
  return parts.find(Boolean);
}

export function validateCard(raw: unknown): ValidateResult<CardProps> {
  if (!isRecord(raw)) return { ok: false, error: "props must be an object" };
  const fields = list(raw.fields)
    .map((item) => isRecord(item) ? { label: str(item.label, 200), value: str(item.value) } : null)
    .filter((item): item is CardProps["fields"][number] => Boolean(item?.label));
  const seen = new Set<string>();
  const actions = list(raw.actions)
    .map((item) => {
      if (!isRecord(item)) return null;
      const openCanvas = str(item.openCanvas, 64);
      const id = str(item.id, 100) || (openCanvas ? `open-canvas-${openCanvas}` : "");
      const label = str(item.label, 100) || id;
      if (!id || seen.has(id)) return null;
      seen.add(id);
      return {
        id,
        label,
        style: item.style === "primary" ? "primary" as const : "default" as const,
        ...(/^[a-z0-9-]{3,64}$/.test(openCanvas) ? { openCanvas } : {}),
      };
    })
    .filter((item): item is CardProps["actions"][number] => item !== null);
  const body = str(raw.body);
  if (!fields.length && !body && !actions.length) return { ok: false, error: "card is empty" };
  const cappedFields = cap(fields, LIMITS.cardFields);
  return {
    ok: true,
    props: { fields: cappedFields.items, body, actions: actions.slice(0, LIMITS.cardActions) },
    ...(cappedFields.truncated ? { truncated: cappedFields.truncated } : {}),
  };
}

export function validateStats(raw: unknown): ValidateResult<StatsProps> {
  if (!isRecord(raw)) return { ok: false, error: "props must be an object" };
  const items = list(raw.items)
    .map((item) => {
      if (!isRecord(item)) return null;
      const label = str(item.label, 200);
      const value = str(item.value, 100);
      if (!label || !value) return null;
      return { label, value, delta: num(item.delta), unit: str(item.unit, 20) };
    })
    .filter((item): item is StatsProps["items"][number] => item !== null);
  if (!items.length) return { ok: false, error: "stats.items is empty" };
  const capped = cap(items, LIMITS.statsItems);
  return { ok: true, props: { items: capped.items }, ...(capped.truncated ? { truncated: capped.truncated } : {}) };
}

function cell(value: unknown): Cell {
  if (value === null || value === undefined) return null;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (typeof value === "boolean") return value;
  if (typeof value === "string") return value.slice(0, LIMITS.text);
  return JSON.stringify(value).slice(0, LIMITS.text);
}

export function validateTable(raw: unknown): ValidateResult<TableProps> {
  if (!isRecord(raw)) return { ok: false, error: "props must be an object" };
  const rawRows = list(raw.rows).filter(isRecord);
  let columns: TableProps["columns"] = list(raw.columns)
    .map((item) => {
      if (typeof item === "string") return item.trim() ? { key: item.trim(), label: item.trim() } : null;
      if (!isRecord(item)) return null;
      const key = str(item.key, 100);
      if (!key) return null;
      return {
        key,
        label: str(item.label, 200) || key,
        align: item.align === "right" ? "right" as const : "left" as const,
        format: FORMATS.includes(item.format as ColumnFormat) ? item.format as ColumnFormat : "text" as const,
      };
    })
    .filter((item): item is NonNullable<typeof item> => item !== null)
    .map((item): TableProps["columns"][number] => ({ align: "left", format: "text", ...item }));
  // 没给列定义时用第一行的键
  if (!columns.length && rawRows.length) {
    columns = Object.keys(rawRows[0]).map((key) => ({ key, label: key, align: "left" as const, format: "text" as const }));
  }
  if (!columns.length) return { ok: false, error: "table.columns is empty" };
  const cappedColumns = cap(columns, LIMITS.tableColumns);
  const cappedRows = cap(rawRows, LIMITS.tableRows);
  const rows = cappedRows.items.map((row) =>
    Object.fromEntries(cappedColumns.items.map((column) => [column.key, cell(row[column.key])])),
  );
  const truncated = mergeTruncation(cappedRows.truncated, cappedColumns.truncated);
  return { ok: true, props: { columns: cappedColumns.items, rows }, ...(truncated ? { truncated } : {}) };
}

export function validateChart(raw: unknown): ValidateResult<ChartProps> {
  if (!isRecord(raw)) return { ok: false, error: "props must be an object" };
  const chartType = raw.chartType === "bar" || raw.chartType === "pie" ? raw.chartType : raw.chartType === "line" ? "line" : null;
  if (!chartType) return { ok: false, error: "chart.chartType must be line / bar / pie" };
  const xKey = str(raw.xKey, 100);
  if (!xKey) return { ok: false, error: "chart.xKey is required" };
  const series = list(raw.series)
    .map((item) => {
      if (typeof item === "string") return item.trim() ? { key: item.trim(), label: item.trim() } : null;
      if (!isRecord(item)) return null;
      const key = str(item.key, 100);
      return key ? { key, label: str(item.label, 200) || key } : null;
    })
    .filter((item): item is ChartProps["series"][number] => item !== null);
  if (!series.length) return { ok: false, error: "chart.series is empty" };
  const cappedSeries = cap(series, LIMITS.chartSeries);
  const keys = cappedSeries.items.map((item) => item.key);
  const data = list(raw.data)
    .filter(isRecord)
    .map((row) => {
      const point: Record<string, string | number> = { [xKey]: str(row[xKey], 200) };
      for (const key of keys) point[key] = num(row[key]) ?? 0;
      return point;
    });
  if (!data.length) return { ok: false, error: "chart.data is empty" };
  const cappedData = cap(data, LIMITS.chartPoints);
  const truncated = mergeTruncation(cappedData.truncated, cappedSeries.truncated);
  return {
    ok: true,
    props: {
      chartType,
      xKey,
      series: chartType === "pie" ? cappedSeries.items.slice(0, 1) : cappedSeries.items,
      data: cappedData.items,
      unit: str(raw.unit, 20),
      stacked: chartType === "bar" && raw.stacked === true,
    },
    ...(truncated ? { truncated } : {}),
  };
}

export function validateForm(raw: unknown): ValidateResult<FormProps> {
  if (!isRecord(raw)) return { ok: false, error: "props must be an object" };
  const names = new Set<string>();
  const fields = list(raw.fields)
    .map((item): FormField | null => {
      if (!isRecord(item)) return null;
      const name = str(item.name, 100);
      if (!name || names.has(name)) return null;
      names.add(name);
      const kind = FORM_KINDS.includes(item.kind as FormFieldKind) ? item.kind as FormFieldKind : "text";
      const options = list(item.options)
        .map((option) => {
          if (typeof option === "string" || typeof option === "number") {
            const value = String(option).trim();
            return value ? { value, label: value } : null;
          }
          if (!isRecord(option)) return null;
          const value = str(option.value, 200);
          return value ? { value, label: str(option.label, 200) || value } : null;
        })
        .filter((option): option is FormField["options"][number] => option !== null)
        .slice(0, LIMITS.formOptions);
      // 选择类没有选项就退成文本框
      const effectiveKind = (kind === "select" || kind === "multiselect") && !options.length ? "text" : kind;
      return {
        name,
        label: str(item.label, 200) || name,
        kind: effectiveKind,
        required: item.required === true,
        default: item.default ?? null,
        placeholder: str(item.placeholder, 200),
        options,
        min: num(item.min),
        max: num(item.max),
        step: num(item.step),
      };
    })
    .filter((item): item is FormField => item !== null);
  if (!fields.length) return { ok: false, error: "form.fields is empty" };
  const capped = cap(fields, LIMITS.formFields);
  return {
    ok: true,
    props: { fields: capped.items, submitLabel: str(raw.submitLabel, 40) },
    ...(capped.truncated ? { truncated: capped.truncated } : {}),
  };
}

/** 表单字段的初始值：先用 default，再按类型补空值 */
export function initialFormValue(field: FormField): unknown {
  const value = field.default;
  switch (field.kind) {
    case "switch":
      return value === true || value === "true";
    case "number":
    case "slider": {
      const parsed = num(value);
      if (parsed !== null) return parsed;
      return field.kind === "slider" ? field.min ?? 0 : null;
    }
    case "multiselect":
      return Array.isArray(value) ? value.map(String).filter((item) => field.options.some((option) => option.value === item)) : [];
    case "select": {
      const text = value === null || value === undefined ? "" : String(value);
      return field.options.some((option) => option.value === text) ? text : "";
    }
    default:
      return value === null || value === undefined ? "" : String(value);
  }
}

/** 必填字段为空时返回该字段 name */
export function missingRequired(fields: FormField[], values: Record<string, unknown>): string | null {
  for (const field of fields) {
    if (!field.required || field.kind === "switch") continue;
    const value = values[field.name];
    const empty = value === null || value === undefined
      || (typeof value === "string" && !value.trim())
      || (Array.isArray(value) && !value.length);
    if (empty) return field.name;
  }
  return null;
}
