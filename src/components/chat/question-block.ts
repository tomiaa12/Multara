// 无头 CLI（claude -p / cursor-agent -p）没法在运行中途等用户回答，
// 所以智能体用一个约定的代码块把问题写在回复末尾，界面把它渲染成输入框上方的选项卡片。
// 字段名沿用 Claude AskUserQuestion 的结构（question / header / options / multiSelect），模型最熟悉。
import { GENUI_FENCE, scanFences, splitGenUi } from "./genui/block";

export const QUESTION_FENCE = "multara-questions";

export interface ChatQuestionOption {
  label: string;
  description?: string;
}

export interface ChatQuestion {
  id: string;
  question: string;
  header?: string;
  options: ChatQuestionOption[];
  multiSelect: boolean;
  allowCustom: boolean;
}

export interface ChatQuestionAnswer {
  selected: string[];
  custom?: string;
}

export interface ExtractedQuestions {
  body: string;
  questions: ChatQuestion[];
  /** 开始围栏已出现、结束围栏还没到：流式输出中途 */
  streaming: boolean;
}

/**
 * genui：生成式界面开关。打开时 ```multara-ui 里 type 为 question 的块也算提问；
 * 关闭时 multara-ui 块一律按代码块显示，不抽成提问卡片。multara-questions 不受开关影响。
 */
export function extractQuestions(text: string, options: { genui?: boolean } = {}): ExtractedQuestions {
  const source = text ?? "";
  const hasUi = Boolean(options.genui) && source.includes(GENUI_FENCE);
  if (!source.includes(QUESTION_FENCE) && !hasUi) return { body: source, questions: [], streaming: false };
  const legacy = extractLegacyQuestions(source);
  if (!hasUi) return legacy;
  const alias = extractUiQuestions(legacy.body);
  if (!alias.questions.length) return legacy;
  return { body: alias.body, questions: alias.questions, streaming: legacy.streaming };
}

/** ```multara-ui 里 type 为 question 的块：等同 multara-questions，抽出来给输入框上方的卡片 */
function extractUiQuestions(source: string): { body: string; questions: ChatQuestion[] } {
  const { segments } = splitGenUi(source);
  let questions: ChatQuestion[] = [];
  const kept: string[] = [];
  for (const segment of segments) {
    if (segment.kind === "md") {
      kept.push(segment.text);
      continue;
    }
    const specs = segment.kind === "ui" ? segment.specs : null;
    if (specs?.length && specs.every((spec) => spec.type === "question")) {
      const parsed = specs.flatMap((spec) => parseQuestions(JSON.stringify(spec.props ?? null)));
      if (parsed.length) {
        questions = parsed;
        continue;
      }
    }
    kept.push(segment.source);
  }
  if (!questions.length) return { body: source, questions };
  return { body: kept.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd(), questions };
}

function extractLegacyQuestions(source: string): ExtractedQuestions {
  if (!source.includes(QUESTION_FENCE)) return { body: source, questions: [], streaming: false };
  let questions: ChatQuestion[] = [];
  let streaming = false;
  const kept: string[] = [];
  for (const segment of scanFences(source, QUESTION_FENCE)) {
    if (segment.kind === "text") {
      kept.push(segment.text);
    } else if (segment.kind === "open") {
      // 流式中途：半截 JSON 不显示
      streaming = true;
    } else {
      // 闭合块一律从正文拿掉；解析失败也不把 JSON 漏给用户
      const parsed = parseQuestions(segment.raw);
      if (parsed.length) questions = parsed;
    }
  }
  return { body: kept.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd(), questions, streaming };
}

export function parseQuestions(raw: string): ChatQuestion[] {
  let value: unknown;
  try {
    value = JSON.parse(raw.trim());
  } catch {
    return [];
  }
  const list = Array.isArray(value)
    ? value
    : isRecord(value) && Array.isArray(value.questions)
      ? value.questions
      : isRecord(value) && typeof value.question === "string"
        ? [value]
        : [];
  return list
    .map((item, index) => normalizeQuestion(item, index))
    .filter((item): item is ChatQuestion => item !== null);
}

function normalizeQuestion(item: unknown, index: number): ChatQuestion | null {
  if (!isRecord(item)) return null;
  const question = text(item.question) || text(item.title) || text(item.prompt);
  if (!question) return null;
  const rawOptions = Array.isArray(item.options) ? item.options : [];
  const options = rawOptions
    .map((option): ChatQuestionOption | null => {
      if (typeof option === "string") return option.trim() ? { label: option.trim() } : null;
      if (!isRecord(option)) return null;
      const label = text(option.label) || text(option.title) || text(option.value);
      if (!label) return null;
      const description = text(option.description);
      return description ? { label, description } : { label };
    })
    .filter((option): option is ChatQuestionOption => option !== null);
  const header = text(item.header);
  return {
    id: text(item.id) || `q${index + 1}`,
    question,
    ...(header ? { header } : {}),
    options,
    multiSelect: item.multiSelect === true || item.multiple === true,
    // 没有选项时只能自由输入；默认始终留一个「其他」入口
    allowCustom: options.length === 0 || item.allowCustom !== false,
  };
}

export function isAnswered(answer: ChatQuestionAnswer | undefined) {
  return Boolean(answer && (answer.selected.length || answer.custom?.trim()));
}

/** 把选择结果写成一条普通用户消息，智能体下一轮按文本读 */
export function formatAnswers(
  questions: ChatQuestion[],
  answers: Record<string, ChatQuestionAnswer | undefined>,
): string {
  const lines = questions.map((question) => {
    const answer = answers[question.id];
    const parts = [...(answer?.selected ?? [])];
    const custom = answer?.custom?.trim();
    if (custom) parts.push(custom);
    const value = parts.length ? parts.join("、") : "（跳过）";
    return questions.length === 1 ? value : `${question.header || question.question}：${value}`;
  });
  return lines.join("\n");
}

function text(value: unknown) {
  return typeof value === "string" ? value.trim() : "";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
