// 智能体在回复里写 ```multara-ui 代码块（JSON），界面原地换成组件。
// 这里只做「切片」：把正文按顺序切成 Markdown 段和界面块段，校验交给 schema.ts。
export const GENUI_FENCE = "multara-ui";
/** 一条回复里最多渲染这么多个界面块，多出来的按普通代码块显示 */
export const GENUI_MAX_BLOCKS = 6;

/** 实验功能「生成式界面」是否打开（app_settings.genuiEnabled） */
export function isGenUiEnabled(settings?: Record<string, string> | null) {
  return settings?.genuiEnabled === "true";
}

export interface GenUiSpec {
  type: string;
  version: number;
  title?: string;
  props: unknown;
}

export type GenUiSegment =
  | { kind: "md"; text: string }
  /** specs 为 null 表示 JSON 解析失败，按代码块降级 */
  | { kind: "ui"; index: number; raw: string; source: string; specs: GenUiSpec[] | null; error?: string }
  /** 开始围栏到了、结束围栏还没到：流式输出中途 */
  | { kind: "pending"; index: number; source: string };

export interface SplitGenUi {
  segments: GenUiSegment[];
  streaming: boolean;
}

interface Fence {
  char: "`" | "~";
  length: number;
}

const FENCE_LINE = /^[ \t]{0,3}(`{3,}|~{3,})(.*)$/;

function openFence(line: string): (Fence & { info: string }) | null {
  const match = FENCE_LINE.exec(line);
  if (!match) return null;
  const marks = match[1];
  const info = match[2].trim();
  // 反引号围栏的 info 里不能再有反引号（CommonMark）
  if (marks[0] === "`" && info.includes("`")) return null;
  return { char: marks[0] as "`" | "~", length: marks.length, info };
}

function closesFence(line: string, fence: Fence) {
  const match = FENCE_LINE.exec(line);
  return Boolean(
    match
      && match[1][0] === fence.char
      && match[1].length >= fence.length
      && !match[2].trim(),
  );
}

export type FenceSegment =
  | { kind: "text"; text: string }
  /** 闭合的目标围栏：raw 是围栏内容，source 是含围栏的原文 */
  | { kind: "block"; raw: string; source: string }
  /** 开始围栏到了、结束围栏还没到（流式中途），source 到文末 */
  | { kind: "open"; source: string };

/**
 * 按行扫描围栏，只认最外层、info 第一个词等于 lang 的代码块；
 * 嵌在其它代码块里的（模型演示协议时）当普通文本。片段按顺序用 "\n" 拼回去等于原文。
 * multara-ui 和 multara-questions 共用这一个扫描。
 */
export function scanFences(text: string, lang: string): FenceSegment[] {
  const lines = text.split("\n");
  const segments: FenceSegment[] = [];
  let pending: string[] = [];
  const flush = () => {
    if (!pending.length) return;
    segments.push({ kind: "text", text: pending.join("\n") });
    pending = [];
  };
  for (let row = 0; row < lines.length; row += 1) {
    const fence = openFence(lines[row]);
    if (!fence) {
      pending.push(lines[row]);
      continue;
    }
    let end = row + 1;
    while (end < lines.length && !closesFence(lines[end], fence)) end += 1;
    const closed = end < lines.length;
    if (fence.info.split(/\s+/)[0] !== lang) {
      // 其它代码块整体原样保留
      const last = closed ? end : lines.length - 1;
      pending.push(...lines.slice(row, last + 1));
      row = last;
      continue;
    }
    flush();
    if (!closed) {
      segments.push({ kind: "open", source: lines.slice(row).join("\n") });
      return segments;
    }
    segments.push({
      kind: "block",
      raw: lines.slice(row + 1, end).join("\n"),
      source: lines.slice(row, end + 1).join("\n"),
    });
    row = end;
  }
  flush();
  return segments;
}

/** 正文按 ```multara-ui 切成 Markdown 段和界面块段 */
export function splitGenUi(text: string): SplitGenUi {
  const source = text ?? "";
  if (!source.includes(GENUI_FENCE)) {
    return { segments: source ? [{ kind: "md", text: source }] : [], streaming: false };
  }
  const segments: GenUiSegment[] = [];
  let index = 0;
  let streaming = false;
  for (const segment of scanFences(source, GENUI_FENCE)) {
    if (segment.kind === "text") {
      segments.push({ kind: "md", text: segment.text });
    } else if (segment.kind === "open") {
      segments.push({ kind: "pending", index, source: segment.source });
      streaming = true;
    } else {
      const parsed = parseSpecs(segment.raw);
      segments.push({
        kind: "ui",
        index,
        raw: segment.raw,
        source: segment.source,
        specs: parsed.specs,
        ...(parsed.error ? { error: parsed.error } : {}),
      });
      index += 1;
    }
  }
  return { segments, streaming };
}

/** 解析一个围栏里的 JSON：单个界面或 {"blocks":[...]} */
export function parseSpecs(raw: string): { specs: GenUiSpec[] | null; error?: string } {
  let value: unknown;
  try {
    value = JSON.parse(raw.trim());
  } catch (cause) {
    return { specs: null, error: cause instanceof Error ? cause.message : String(cause) };
  }
  const list = isRecord(value) && Array.isArray(value.blocks)
    ? value.blocks
    : Array.isArray(value)
      ? value
      : [value];
  const specs = list.map(toSpec);
  if (!specs.length || specs.some((spec) => spec === null)) {
    return { specs: null, error: "missing type" };
  }
  return { specs: specs as GenUiSpec[] };
}

function toSpec(value: unknown): GenUiSpec | null {
  if (!isRecord(value) || typeof value.type !== "string" || !value.type.trim()) return null;
  const version = typeof value.version === "number" && Number.isFinite(value.version) && value.version >= 1
    ? Math.floor(value.version)
    : 1;
  const title = typeof value.title === "string" && value.title.trim() ? value.title.trim() : undefined;
  return {
    type: value.type.trim(),
    version,
    ...(title ? { title } : {}),
    props: value.props ?? {},
  };
}

/** 界面块的稳定 id：消息 id + 片段序号（+ blocks 内序号） */
export function genUiBlockId(messageId: string, index: number, sub = 0) {
  return sub ? `${messageId}-b${index}.${sub}` : `${messageId}-b${index}`;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
