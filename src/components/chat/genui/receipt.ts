import type { ChatMessage } from "../../../domain/types";
import { t } from "../../../i18n";
import { splitGenUi, genUiBlockId, isRecord } from "./block";
import { genUiEntry } from "./registry";
import type { CardProps, FormField, FormProps } from "./schema";

export type GenUiReceipt = {
  title: string;
  actionLabel: string;
  rows: { label: string; values: string[] }[];
};

/** 界面提交在对话里的展示。发给模型的原文仍是带 JSON 的那一行。 */
export function genUiReceipt(
  message: Pick<ChatMessage, "content" | "sourceType" | "sourceMeta">,
  assistantContent?: string,
): GenUiReceipt | null {
  if (message.sourceType !== "genui") return null;
  const meta = message.sourceMeta ?? {};
  const action = typeof meta.action === "string" ? meta.action : "";
  const spec = specFor(assistantContent, meta);
  const title = text(meta.title) || titleFromSpec(spec) || titleFromContent(message.content);
  const fields = formFields(spec);
  const values = isRecord(meta.values) ? meta.values : null;
  return {
    title,
    actionLabel: actionLabel(action, spec),
    rows: values ? rowsFor(values, fields) : [],
  };
}

function specFor(content: string | undefined, meta: Record<string, unknown>) {
  const messageId = text(meta.messageId);
  const blockId = text(meta.blockId);
  if (!content || !messageId || !blockId.startsWith(`${messageId}-b`)) return null;
  const rest = blockId.slice(messageId.length + 2);
  const [indexText, subText] = rest.split(".");
  const index = Number(indexText);
  const sub = subText ? Number(subText) : 0;
  if (!Number.isInteger(index) || index < 0) return null;
  const segment = splitGenUi(content).segments.find((item) => item.kind === "ui" && item.index === index);
  if (!segment || segment.kind !== "ui") return null;
  const spec = segment.specs?.[sub];
  if (!spec || genUiBlockId(messageId, index, sub) !== blockId) return null;
  return spec;
}

function formFields(spec: ReturnType<typeof specFor>): FormField[] {
  if (!spec || spec.type !== "form") return [];
  const result = genUiEntry("form")?.validate(spec.props);
  return result?.ok ? (result.props as FormProps).fields : [];
}

function titleFromSpec(spec: ReturnType<typeof specFor>): string {
  return spec?.title?.trim() ?? "";
}

function titleFromContent(content: string): string {
  const line = content.split("\n")[0] ?? "";
  const match = line.match(/^\[[^\]]* · ([^\]]+)\]/);
  return match?.[1]?.trim() ?? "";
}

function actionLabel(action: string, spec: ReturnType<typeof specFor>): string {
  if (action === "submit") return t("genui.submitted");
  if (spec?.type === "card") {
    const result = genUiEntry("card")?.validate(spec.props);
    if (result?.ok) {
      const hit = (result.props as CardProps).actions.find((item) => item.id === action);
      if (hit?.label) return hit.label;
    }
  }
  return action;
}

function rowsFor(values: Record<string, unknown>, fields: FormField[]): GenUiReceipt["rows"] {
  const used = new Set<string>();
  const rows: GenUiReceipt["rows"] = [];
  const push = (name: string, label: string, field?: FormField) => {
    if (!Object.prototype.hasOwnProperty.call(values, name)) return;
    used.add(name);
    const shown = displayValues(values[name], field);
    if (!shown.length) return;
    rows.push({ label, values: shown });
  };
  for (const field of fields) push(field.name, field.label, field);
  for (const name of Object.keys(values)) {
    if (!used.has(name)) push(name, name);
  }
  return rows;
}

function displayValues(value: unknown, field?: FormField): string[] {
  if (Array.isArray(value)) {
    return value.map((item) => optionLabel(field, String(item))).filter(Boolean);
  }
  if (typeof value === "boolean") return [value ? t("genui.yes") : t("genui.no")];
  if (value == null || value === "") return [];
  return [optionLabel(field, String(value))];
}

function optionLabel(field: FormField | undefined, value: string): string {
  return field?.options.find((option) => option.value === value)?.label || value;
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}
