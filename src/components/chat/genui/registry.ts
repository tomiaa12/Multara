import { defineAsyncComponent, type Component } from "vue";
import { parseQuestions, type ChatQuestion } from "../question-block";
import {
  validateCard,
  validateChart,
  validateForm,
  validateStats,
  validateTable,
  type CardProps,
  type ChartProps,
  type FormProps,
  type StatsProps,
  type TableProps,
  type ValidateResult,
} from "./schema";
import { cardToText, chartToText, formToText, statsToText, tableToText } from "./to-text";
import { t } from "../../../i18n";

export interface GenUiEntry {
  /** 显示名的文案键：块没写标题时，提交消息的第一行用它 */
  label: string;
  validate: (props: unknown) => ValidateResult<unknown>;
  /** null：不在正文里渲染（question 走输入框上方的提问卡片） */
  component: Component | null;
  /** 有提交 / 按钮操作，提交后变只读 */
  interactive: boolean;
  /** 校验通过后的 props 转成纯文字（只认文字的地方用） */
  toText: (props: unknown) => string;
}

/** 校验并转文字；类型不认识或校验失败返回 null */
export function genUiToText(type: string, rawProps: unknown): string | null {
  const entry = registry.get(type);
  const result = entry?.validate(rawProps);
  return entry && result?.ok ? entry.toText(result.props) : null;
}

const registry = new Map<string, GenUiEntry>();

export function registerGenUi(type: string, entry: GenUiEntry) {
  registry.set(type, entry);
}

export function genUiEntry(type: string): GenUiEntry | undefined {
  return registry.get(type);
}

/** 类型的显示名（「表格」「表单」）；不认识的类型原样返回 */
export function genUiLabel(type: string): string {
  const entry = registry.get(type);
  return entry ? t(entry.label) : type;
}

registerGenUi("card", {
  label: "genui.type.card",
  validate: validateCard,
  component: defineAsyncComponent(() => import("./components/GenUiCard.vue")),
  interactive: true,
  toText: (props) => cardToText(props as CardProps),
});
registerGenUi("stats", {
  label: "genui.type.stats",
  validate: validateStats,
  component: defineAsyncComponent(() => import("./components/GenUiStats.vue")),
  interactive: false,
  toText: (props) => statsToText(props as StatsProps),
});
registerGenUi("table", {
  label: "genui.type.table",
  validate: validateTable,
  component: defineAsyncComponent(() => import("./components/GenUiTable.vue")),
  interactive: false,
  toText: (props) => tableToText(props as TableProps),
});
// 图表代码只在出现 chart 块时加载
registerGenUi("chart", {
  label: "genui.type.chart",
  validate: validateChart,
  component: defineAsyncComponent(() => import("./components/GenUiChart.vue")),
  interactive: false,
  toText: (props) => chartToText(props as ChartProps),
});
registerGenUi("form", {
  label: "genui.type.form",
  validate: validateForm,
  component: defineAsyncComponent(() => import("./components/GenUiForm.vue")),
  interactive: true,
  toText: (props) => formToText(props as FormProps),
});
// multara-questions 的别名：props 与提问块的 JSON 相同
registerGenUi("question", {
  label: "genui.type.question",
  validate: (props) => {
    const questions = parseQuestions(JSON.stringify(props ?? null));
    return questions.length ? { ok: true, props: questions } : { ok: false, error: "no questions" };
  },
  component: null,
  interactive: true,
  toText: (props) => (props as ChatQuestion[])
    .map((question) => [question.question, ...question.options.map((option) => `- ${option.label}`)].join("\n"))
    .join("\n"),
});
