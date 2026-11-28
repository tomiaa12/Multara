import type { RawRunDetail, RawRunEvent } from "../../services/backend";

export const PROMPT_PREVIEW_CHARS = 50_000;

/** 命令行展示用：只给含空格/引号的参数加引号，避免把可读的 argv 变成一整段 JSON。 */
export function shellQuote(arg: string): string {
  if (arg === "") return "''";
  if (/^[A-Za-z0-9_@%+=:,./-]+$/.test(arg)) return arg;
  return `'${arg.replace(/'/g, `'\\''`)}'`;
}

export function formatCommandLine(program: string, args: string[]): string {
  return [program, ...args].map(shellQuote).join(" ");
}

/** 事件分页追加：按 seq 去重并排序，避免重复点“加载更多”时插出重复行。 */
export function appendEvents(existing: RawRunEvent[], incoming: RawRunEvent[]): RawRunEvent[] {
  const bySeq = new Map<number, RawRunEvent>();
  for (const event of existing) bySeq.set(event.seq, event);
  for (const event of incoming) bySeq.set(event.seq, event);
  return [...bySeq.values()].sort((left, right) => left.seq - right.seq);
}

export function searchableEventText(event: RawRunEvent): string {
  return [
    event.eventType,
    event.tool,
    event.callId,
    event.content,
    event.inputJson,
    event.output,
    event.outputJson,
    event.status,
  ]
    .filter((value): value is string => Boolean(value))
    .join("\n")
    .toLowerCase();
}

export function previewText(text: string, limit = PROMPT_PREVIEW_CHARS) {
  if (text.length <= limit) return { text, clipped: false };
  return { text: text.slice(0, limit), clipped: true };
}

export function buildRunJson(detail: RawRunDetail) {
  return {
    run: detail.run,
    invocation: detail.invocation,
    userMessage: detail.userMessage,
    assistantMessages: detail.assistantMessages,
    events: appendEvents([], detail.events),
  };
}
