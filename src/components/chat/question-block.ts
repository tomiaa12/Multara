// 无头 CLI（claude -p / cursor-agent -p）没法在运行中途等用户回答，
// 所以智能体用一个约定的代码块把问题写在回复末尾，界面把它渲染成输入框上方的选项卡片。
// 字段名沿用 Claude AskUserQuestion 的结构（question / header / options / multiSelect），模型最熟悉。
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

// 匹配完整的提问块；info 允许带空格，结尾围栏可以没有换行
const CLOSED_BLOCK = new RegExp("(^|\\n)[ \\t]*```[ \\t]*" + QUESTION_FENCE + "[^\\n]*\\n([\\s\\S]*?)\\n[ \\t]*```[ \\t]*(?=\\n|$)", "g");
const OPEN_BLOCK = new RegExp("(^|\\n)[ \\t]*```[ \\t]*" + QUESTION_FENCE + "[^\\n]*(\\n[\\s\\S]*)?$");

export function extractQuestions(text: string): ExtractedQuestions {
  const source = text ?? "";
  if (!source.includes(QUESTION_FENCE)) return { body: source, questions: [], streaming: false };
  let questions: ChatQuestion[] = [];
  let body = source.replace(CLOSED_BLOCK, (_match, lead: string, json: string) => {
    const parsed = parseQuestions(json);
    if (parsed.length) questions = parsed;
    return lead;
  });
  let streaming = false;
  const open = OPEN_BLOCK.exec(body);
  if (open) {
    streaming = true;
    body = body.slice(0, open.index + open[1].length);
  }
  return { body: body.replace(/\n{3,}/g, "\n\n").trimEnd(), questions, streaming };
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
