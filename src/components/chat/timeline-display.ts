import type { TimelineItem } from "../../domain/types";
import { resolveLocale, translate, type MessageKey } from "../../i18n";

export type ToolKind = "read" | "search" | "browser" | "edit" | "write" | "shell" | "other";

export interface DiffLine {
  kind: "context" | "add" | "del";
  oldLine?: number;
  newLine?: number;
  text: string;
}

export interface PairedTool {
  use: TimelineItem;
  result?: TimelineItem;
  children?: TimelineBlock[];
}

export interface PlanTodo {
  id: string;
  text: string;
  status: "pending" | "in_progress" | "completed" | "cancelled" | string;
}

export type ExploreEntry =
  | { kind: "tool"; item: PairedTool }
  | { kind: "thinking"; item: TimelineItem };

export type TimelineBlock =
  | { type: "thinking"; item: TimelineItem }
  | { type: "error"; item: TimelineItem }
  | { type: "explore"; items: ExploreEntry[] }
  | { type: "edit"; item: PairedTool }
  | { type: "shell"; item: PairedTool }
  | { type: "tool"; item: PairedTool }
  | { type: "plan"; todos: PlanTodo[] }
  | { type: "permission"; item: TimelineItem };

export interface FileRevealTarget {
  path: string;
  startLine?: number;
  endLine?: number;
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

export function resolvedInput(item: TimelineItem | undefined): Record<string, unknown> {
  if (!item) return {};
  const direct = asRecord(item.input);
  if (Object.keys(direct).length) return direct;
  const raw = item.content?.trim();
  if (!raw) return {};
  try {
    return asRecord(JSON.parse(raw));
  } catch {
    return {};
  }
}

export function inputRecord(item: TimelineItem | undefined): Record<string, unknown> {
  return resolvedInput(item);
}

export function toolKind(name: string | null | undefined): ToolKind {
  const n = (name ?? "").toLowerCase().replaceAll(/[^a-z]/g, "");
  if (!n) return "other";
  if (["read", "readfile", "readpath", "viewfile"].some((tool) => n.includes(tool))) return "read";
  if (["grep", "glob", "search", "listdir", "listdirectory", "find", "explore", "semanticsearch"].some((tool) => n.includes(tool))) {
    return "search";
  }
  if (["browser", "snapshot", "navigate", "webfetch", "websearch", "puppeteer", "playwright"].some((tool) => n.includes(tool))) {
    return "browser";
  }
  if (["edit", "strreplace", "searchreplace", "applypatch", "editfile", "notebookedit"].some((tool) => n.includes(tool))) return "edit";
  if (["write", "writefile", "createfile"].some((tool) => n.includes(tool))) return "write";
  if (n === "run" || n === "bash" || ["shell", "terminal", "command"].some((tool) => n.includes(tool))) return "shell";
  return "other";
}

export function inferToolName(item: TimelineItem): string {
  if (item.tool?.trim()) return item.tool.trim();
  const input = resolvedInput(item);
  if (input.old_string || input.oldString || input.new_string || input.newString) return "Edit";
  if (input.fileText || input.file_text || input.contents) return "Write";
  if (input.command || input.cmd) return "Bash";
  if (input.pattern) return "Grep";
  if (input.glob || input.targetDirectory) return "Glob";
  if (input.url) return "WebFetch";
  if (input.path || input.file_path || input.filePath || input.target_file) return "Read";
  return "";
}

export function itemKind(item: TimelineItem): ToolKind {
  const inferred = inferToolName(item);
  const kind = toolKind(inferred);
  if (kind !== "other") return kind;
  const input = resolvedInput(item);
  if (input.old_string || input.oldString || input.new_string || input.newString) return "edit";
  if (input.fileText || input.file_text || input.contents) return "write";
  if (input.command || input.cmd) return "shell";
  if (input.pattern || input.glob || input.query) return "search";
  if (input.url) return "browser";
  if (input.path || input.file_path || input.filePath || input.target_file) return "read";
  return "other";
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function stringField(input: Record<string, unknown>, keys: string[]): string {
  for (const key of keys) {
    const value = input[key];
    if (typeof value === "string" && value.trim()) return value;
  }
  return "";
}

function numberField(input: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = input[key];
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim() && Number.isFinite(Number(value))) return Number(value);
  }
  return undefined;
}

export function toolPath(item: TimelineItem | undefined): string {
  return stringField(resolvedInput(item), ["path", "file_path", "filePath", "target_file", "file", "uri"]);
}

export function toolCommand(item: TimelineItem | undefined): string {
  return stringField(resolvedInput(item), ["command", "cmd", "script", "code"]);
}

export function toolDescription(item: TimelineItem | undefined): string {
  return stringField(resolvedInput(item), ["description", "title", "purpose"]);
}

function shellTextFromValue(value: unknown): string {
  if (typeof value === "string" && value.trim()) return value.trim();
  const record = asRecord(value);
  const nested = [record.success, record.result, record];
  const keys = ["interleavedOutput", "interleaved_output", "stdout", "output", "stderr", "content"];
  for (const source of nested) {
    if (typeof source === "string" && source.trim()) return source.trim();
    const fields = asRecord(source);
    for (const key of keys) {
      const text = fields[key];
      if (typeof text === "string" && text.trim()) return text.trim();
    }
  }
  return "";
}

export function shellResultText(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return trimmed;
  try {
    const text = shellTextFromValue(JSON.parse(trimmed) as unknown);
    return text || trimmed;
  } catch {
    return trimmed;
  }
}

export function shellTranscript(command: string, output: string): string {
  const commandText = command.trim();
  const commandBlock = commandText
    ? commandText.split("\n").map((line, index) => (index === 0 ? `$ ${line}` : line)).join("\n")
    : "";
  return [commandBlock, output.trim()].filter(Boolean).join("\n");
}

export function toolLineRange(item: TimelineItem | undefined): { start?: number; end?: number } {
  const input = resolvedInput(item);
  const start = numberField(input, ["offset", "start_line", "startLine", "line", "from"]);
  const end = numberField(input, ["end_line", "endLine", "to"]);
  const limit = numberField(input, ["limit", "count"]);
  if (start != null && end != null) return { start, end };
  if (start != null && limit != null) return { start, end: start + limit - 1 };
  if (start != null) return { start };
  return {};
}

export function lineRangeLabel(item: TimelineItem | undefined): string {
  const range = toolLineRange(item);
  if (range.start != null && range.end != null && range.end !== range.start) return `L${range.start}-${range.end}`;
  if (range.start != null) return `L${range.start}`;
  return "";
}

export function readLabel(item: TimelineItem): string {
  const path = fileName(toolPath(item) || "file");
  const range = lineRangeLabel(item);
  return range ? `${path} ${range}` : path;
}

export function searchLabel(item: TimelineItem): string {
  const input = resolvedInput(item);
  return stringField(input, ["pattern", "query", "glob", "search", "url"]) || fileName(toolPath(item) || "");
}

export function browserLabel(item: TimelineItem): string {
  const input = resolvedInput(item);
  return stringField(input, ["url", "query", "title", "selector", "text"]) || inferToolName(item);
}

export function exploreVerb(item: TimelineItem, language?: string): string {
  const kind = itemKind(item);
  const name = inferToolName(item);
  const lower = name.toLowerCase();
  if (kind === "read") return translate("chat.read", language);
  if (kind === "search") {
    if (lower.includes("glob")) return translate("chat.globbed", language);
    if (lower.includes("list")) return translate("chat.listed", language);
    return translate("chat.grepped", language);
  }
  if (kind === "browser") {
    if (lower.includes("snapshot")) return translate("chat.snapshot", language);
    if (lower.includes("navigate")) return translate("chat.opened", language);
    if (lower.includes("click")) return translate("chat.clicked", language);
    if (lower.includes("fetch") || lower.includes("search")) return translate("chat.fetched", language);
    return name || translate("chat.browser", language);
  }
  return name || translate("chat.tool", language);
}

export function exploreDetail(item: TimelineItem): string {
  const kind = itemKind(item);
  if (kind === "read") return readLabel(item);
  if (kind === "search") return searchLabel(item);
  if (kind === "browser") return browserLabel(item);
  return toolPath(item) ? readLabel(item) : searchLabel(item) || inferToolName(item);
}

export function exploreTitle(items: ExploreEntry[], language?: string): string {
  const tools = items.flatMap((entry) => (entry.kind === "tool" ? [entry.item] : []));
  const files = new Set<string>();
  let searches = 0;
  let browsers = 0;
  for (const item of tools) {
    const kind = itemKind(item.use);
    if (kind === "search") searches += 1;
    else if (kind === "browser") browsers += 1;
    else files.add(toolPath(item.use) || readLabel(item.use));
  }
  if (resolveLocale(language) === "zh-CN") {
    const parts: string[] = [];
    if (files.size) parts.push(`${files.size} ${translate("chat.fileUnit", language)}`);
    if (searches) parts.push(`${searches} ${translate("chat.searchUnit", language)}`);
    if (browsers) parts.push(`${browsers} ${translate("chat.browserActionUnit", language)}`);
    if (!parts.length) return `${translate("chat.explored", language)} ${tools.length} ${translate("chat.actionUnit", language)}`;
    return `${translate("chat.explored", language)} ${parts.join("、")}`;
  }
  const parts: string[] = [];
  if (files.size) parts.push(`${files.size} file${files.size === 1 ? "" : "s"}`);
  if (searches) parts.push(`${searches} search${searches === 1 ? "" : "es"}`);
  if (browsers) parts.push(`${browsers} browser action${browsers === 1 ? "" : "s"}`);
  if (!parts.length) return `Explored ${tools.length} action${tools.length === 1 ? "" : "s"}`;
  return `Explored ${parts.join(", ")}`;
}

export function fileRevealTarget(item: TimelineItem): FileRevealTarget | null {
  const path = toolPath(item);
  if (!path) return null;
  const range = toolLineRange(item);
  return { path, startLine: range.start, endLine: range.end };
}

export function thinkingSummary(item: TimelineItem, running?: boolean, language?: string): string {
  if (running) return translate("chat.thinking", language);
  const text = (item.content ?? "").trim();
  if (!text || text.length < 120) return translate("chat.thoughtBriefly", language);
  return translate("chat.thought", language);
}

export function workFoldLabel(elapsedMs?: number | null, language?: string): string {
  if (elapsedMs == null) return translate("chat.worked", language);
  const seconds = Math.max(1, Math.round(elapsedMs / 1000));
  return translate("chat.workedFor", language).replace("{seconds}", String(seconds));
}

function formatLiveElapsed(seconds: number, language?: string): string {
  const elapsed = Math.max(0, Math.floor(seconds));
  if (elapsed < 60) return translate("chat.elapsedSec", language, { n: elapsed });
  const minutes = Math.floor(elapsed / 60);
  const rest = elapsed % 60;
  return rest
    ? translate("chat.elapsedMinSec", language, { m: minutes, s: rest })
    : translate("chat.elapsedMin", language, { n: minutes });
}

function liveStageKey(items: TimelineItem[], status?: "queued" | "running" | "sending" | null): MessageKey {
  if (status === "queued") return "chat.queued";
  let latest: TimelineItem | undefined;
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (!item || item.eventType === "error" || item.eventType === "tool_result") continue;
    latest = item;
    break;
  }
  if (!latest) return status === "sending" ? "chat.waiting" : "chat.thinking";
  if (latest.eventType === "text") return "chat.typing";
  if (latest.eventType === "thinking") return "chat.thinking";
  const kind = toolKind(inferToolName(latest) || latest.tool);
  if (kind === "shell") return "chat.runningCommand";
  if (kind === "read") return "chat.readingFiles";
  if (kind === "search") return "chat.searchingCode";
  if (kind === "edit" || kind === "write") return "chat.makingEdits";
  if (kind === "browser") return "chat.searchingWeb";
  return "chat.working";
}

function isEditItem(item: TimelineItem): boolean {
  const kind = toolKind(inferToolName(item) || item.tool);
  return kind === "edit" || kind === "write";
}

function editFileLabel(name: string, language?: string): string {
  return translate("chat.editingFile", language).replace("{file}", name);
}

// 还没有结果的修改都算正在进行。同一轮可以同时改多个文件，每个文件一条状态。
function pendingEditNames(items: TimelineItem[]): string[] {
  const finished = new Set(
    items
      .filter((item) => item.eventType === "tool_result" && item.callId)
      .map((item) => item.callId as string),
  );
  const names: string[] = [];
  const seen = new Set<string>();
  for (const item of items) {
    if (item.eventType !== "tool_use" || !isEditItem(item)) continue;
    if (item.callId && finished.has(item.callId)) continue;
    const name = fileName(toolPath(item));
    if (!name || seen.has(name)) continue;
    seen.add(name);
    names.push(name);
  }
  return names;
}

function latestEditName(items: TimelineItem[]): string {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (!item || item.eventType === "error" || item.eventType === "tool_result") continue;
    if (!isEditItem(item)) return "";
    return fileName(toolPath(item));
  }
  return "";
}

/** 运行状态行拆成「阶段文案」和「已过时间」，界面只对阶段文案做扫光和轮播 */
export function liveWorkParts(
  items: TimelineItem[],
  elapsedSeconds: number,
  options?: { language?: string; status?: "queued" | "running" | "sending" | null },
): Array<{ label: string; elapsed: string }> {
  const elapsed = formatLiveElapsed(elapsedSeconds, options?.language);
  const stage = liveStageKey(items, options?.status);
  if (stage === "chat.makingEdits") {
    const pending = pendingEditNames(items);
    const names = pending.length ? pending : [latestEditName(items)].filter(Boolean);
    if (names.length) return names.map((name) => ({ label: editFileLabel(name, options?.language), elapsed }));
  }
  return [{ label: translate(stage, options?.language), elapsed }];
}

export function liveWorkLabels(
  items: TimelineItem[],
  elapsedSeconds: number,
  options?: { language?: string; status?: "queued" | "running" | "sending" | null },
): string[] {
  return liveWorkParts(items, elapsedSeconds, options).map(({ label, elapsed }) => `${label} · ${elapsed}`);
}

export function liveWorkLabel(
  items: TimelineItem[],
  elapsedSeconds: number,
  options?: { language?: string; status?: "queued" | "running" | "sending" | null },
): string {
  return liveWorkLabels(items, elapsedSeconds, options).join("\n");
}

function hydrateTool(item: TimelineItem): TimelineItem {
  const input = resolvedInput(item);
  const tool = inferToolName(item) || item.tool;
  return {
    ...item,
    tool: tool || item.tool,
    input: Object.keys(input).length ? input : item.input,
  };
}

function canMergeToolUse(previous: TimelineItem, item: TimelineItem): boolean {
  if (previous.eventType !== "tool_use" || item.eventType !== "tool_use") return false;
  if (previous.callId && item.callId) return previous.callId === item.callId;
  return !item.tool && !Object.keys(asRecord(item.input)).length;
}

function coalesceTimeline(items: TimelineItem[]): TimelineItem[] {
  const merged: TimelineItem[] = [];
  for (const raw of items) {
    const item = hydrateTool(raw);
    const previous = merged.at(-1);
    if (previous && canMergeToolUse(previous, item)) {
      const content = `${previous.content ?? ""}${item.content ?? ""}`;
      merged[merged.length - 1] = hydrateTool({
        ...previous,
        tool: previous.tool || item.tool,
        callId: previous.callId || item.callId,
        content,
        input: item.input ?? previous.input,
      });
      continue;
    }
    merged.push(item);
  }
  return merged;
}

export function parseUnifiedDiff(diff: string): DiffLine[] {
  const lines: DiffLine[] = [];
  let oldLine = 0;
  let newLine = 0;
  for (const raw of diff.replaceAll("\r\n", "\n").split("\n")) {
    if (raw.startsWith("@@")) {
      const match = /@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(raw);
      if (match) {
        oldLine = Number(match[1]);
        newLine = Number(match[2]);
      }
      continue;
    }
    if (
      raw.startsWith("diff ") ||
      raw.startsWith("index ") ||
      raw.startsWith("---") ||
      raw.startsWith("+++") ||
      raw.startsWith("***")
    ) {
      continue;
    }
    if (raw.startsWith("+")) {
      lines.push({ kind: "add", newLine, text: raw.slice(1) });
      newLine += 1;
    } else if (raw.startsWith("-")) {
      lines.push({ kind: "del", oldLine, text: raw.slice(1) });
      oldLine += 1;
    } else if (raw.startsWith("\\")) {
      continue;
    } else if (raw.length) {
      const text = raw.startsWith(" ") ? raw.slice(1) : raw;
      lines.push({ kind: "context", oldLine, newLine, text });
      oldLine += 1;
      newLine += 1;
    }
  }
  return lines;
}

export function lineDiff(before: string, after: string): DiffLine[] {
  const a = before.replaceAll("\r\n", "\n").split("\n");
  const b = after.replaceAll("\r\n", "\n").split("\n");
  if (a.length * b.length > 80_000) {
    return [
      ...a.map((text, index) => ({ kind: "del" as const, oldLine: index + 1, text })),
      ...b.map((text, index) => ({ kind: "add" as const, newLine: index + 1, text })),
    ];
  }
  const dp = Array.from({ length: a.length + 1 }, () => Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const lines: DiffLine[] = [];
  let i = 0;
  let j = 0;
  let oldLine = 1;
  let newLine = 1;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      lines.push({ kind: "context", oldLine, newLine, text: a[i] });
      i += 1;
      j += 1;
      oldLine += 1;
      newLine += 1;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      lines.push({ kind: "del", oldLine, text: a[i] });
      i += 1;
      oldLine += 1;
    } else {
      lines.push({ kind: "add", newLine, text: b[j] });
      j += 1;
      newLine += 1;
    }
  }
  while (i < a.length) {
    lines.push({ kind: "del", oldLine, text: a[i] });
    i += 1;
    oldLine += 1;
  }
  while (j < b.length) {
    lines.push({ kind: "add", newLine, text: b[j] });
    j += 1;
    newLine += 1;
  }
  return lines;
}

export function diffStats(lines: DiffLine[]): { added: number; deleted: number } {
  return {
    added: lines.filter((line) => line.kind === "add").length,
    deleted: lines.filter((line) => line.kind === "del").length,
  };
}

export const DIFF_COLLAPSE_LINES = 4;

export function withLeadingContext(lines: DiffLine[]): DiffLine[] {
  const firstChange = lines.findIndex((line) => line.kind !== "context");
  if (firstChange > 1) return lines.slice(firstChange - 1);
  if (!lines.length || firstChange !== 0) return lines;
  return [{ kind: "context", text: "" }, ...lines];
}

function looksLikeUnifiedDiff(text: string): boolean {
  return text.includes("@@") || text.startsWith("diff ") || text.startsWith("---");
}

function diffFromResult(result?: TimelineItem): DiffLine[] {
  const output = result?.output || result?.content || "";
  if (!output.trim()) return [];
  try {
    const parsed = JSON.parse(output) as unknown;
    const root = asRecord(parsed);
    const success = asRecord(root.success);
    const diff = stringField(success, ["diffString", "diff_string", "diff", "patch"])
      || stringField(root, ["diffString", "diff_string", "diff", "patch"]);
    if (diff) return withLeadingContext(parseUnifiedDiff(diff));
  } catch {
    if (looksLikeUnifiedDiff(output)) return withLeadingContext(parseUnifiedDiff(output));
  }
  if (looksLikeUnifiedDiff(output)) return withLeadingContext(parseUnifiedDiff(output));
  return [];
}

export function editDiff(item: PairedTool): DiffLine[] {
  const input = resolvedInput(item.use);
  const before = stringField(input, ["old_string", "oldString", "oldText", "before"]);
  const after = stringField(input, ["new_string", "newString", "newText", "after"]);
  if (before || after) return withLeadingContext(lineDiff(before, after));
  const patch = stringField(input, ["patch", "diff", "contents", "content", "fileText", "file_text"]);
  if (looksLikeUnifiedDiff(patch)) return withLeadingContext(parseUnifiedDiff(patch));
  const fromResult = diffFromResult(item.result);
  if (fromResult.length) return fromResult;
  if ((itemKind(item.use) === "write" || itemKind(item.use) === "edit") && patch) {
    return withLeadingContext(patch.split("\n").map((text, index) => ({ kind: "add" as const, newLine: index + 1, text })));
  }
  return [];
}

function isExploreKind(kind: ToolKind): boolean {
  return kind === "read" || kind === "search" || kind === "browser";
}

function isBlankTool(item: TimelineItem): boolean {
  return !inferToolName(item) && !toolPath(item) && !toolCommand(item) && Object.keys(resolvedInput(item)).length === 0;
}

function todosFromInput(input: Record<string, unknown>): PlanTodo[] {
  const list = Array.isArray(input.todos)
    ? input.todos
    : Array.isArray(input.entries)
      ? input.entries
      : [];
  return list.flatMap((entry, index) => {
    const record = asRecord(entry);
    const text = stringField(record, ["content", "text", "title", "description"]);
    if (!text) return [];
    return [{
      id: stringField(record, ["id"]) || String(index),
      text,
      status: stringField(record, ["status"]) || "pending",
    }];
  });
}

export function pinnedTodos(items: TimelineItem[]): PlanTodo[] {
  let latest: PlanTodo[] = [];
  for (const item of coalesceTimeline(items)) {
    if (item.eventType === "plan") {
      latest = todosFromInput(resolvedInput(item));
      continue;
    }
    const name = inferToolName(item).toLowerCase();
    if (item.eventType === "tool_use" && (name === "todowrite" || name.includes("todo"))) {
      latest = todosFromInput(resolvedInput(item));
    }
  }
  return latest;
}

export function permissionOptions(item: TimelineItem): Array<{ id: string; label: string; kind?: string }> {
  const input = resolvedInput(item);
  const options = Array.isArray(input.options) ? input.options : [];
  return options.flatMap((option) => {
    const record = asRecord(option);
    const id = stringField(record, ["optionId", "id"]);
    if (!id) return [];
    return [{
      id,
      label: stringField(record, ["name", "label", "optionId"]) || id,
      kind: stringField(record, ["kind"]) || undefined,
    }];
  });
}

function withChildren(pair: PairedTool, nested: Map<string, TimelineItem[]>): PairedTool {
  const callId = pair.use.callId;
  if (!callId) return pair;
  const kids = nested.get(callId);
  if (!kids?.length) return pair;
  return { ...pair, children: groupTimeline(kids) };
}

export function groupTimeline(items: TimelineItem[]): TimelineBlock[] {
  const process = coalesceTimeline(items).filter((item) => item.eventType !== "text");
  const nested = new Map<string, TimelineItem[]>();
  const top: TimelineItem[] = [];
  const callIds = new Set(process.map((item) => item.callId).filter((id): id is string => Boolean(id)));
  for (const item of process) {
    const parent = item.parentCallId?.trim();
    if (parent && callIds.has(parent)) {
      const list = nested.get(parent) ?? [];
      list.push(item);
      nested.set(parent, list);
    } else {
      top.push(item);
    }
  }
  const results = top.filter((item) => item.eventType === "tool_result");
  const used = new Set<number>();
  const paired: TimelineBlock[] = [];

  for (const item of top) {
    if (item.eventType === "plan") {
      const todos = todosFromInput(resolvedInput(item));
      if (todos.length) paired.push({ type: "plan", todos });
      continue;
    }
    if (item.eventType === "permission") {
      paired.push({ type: "permission", item });
      continue;
    }
    if (item.eventType === "thinking") {
      if (!(item.content ?? "").trim()) continue;
      paired.push({ type: "thinking", item });
      continue;
    }
    if (item.eventType === "error") {
      if (!(item.content || item.output || "").trim()) continue;
      paired.push({ type: "error", item });
      continue;
    }
    if (item.eventType !== "tool_use") continue;
    if (isBlankTool(item)) continue;
    const name = inferToolName(item).toLowerCase();
    if (name === "todowrite" || name.includes("todo")) {
      const todos = todosFromInput(resolvedInput(item));
      if (todos.length) paired.push({ type: "plan", todos });
      continue;
    }
    const result = results.find((candidate) => {
      if (used.has(candidate.seq)) return false;
      if (item.callId && candidate.callId) return candidate.callId === item.callId;
      return false;
    });
    if (result) used.add(result.seq);
    const pair = withChildren({ use: item, result }, nested);
    const kind = itemKind(item);
    if (isExploreKind(kind)) paired.push({ type: "explore", items: [{ kind: "tool", item: pair }] });
    else if (kind === "edit" || kind === "write") paired.push({ type: "edit", item: pair });
    else if (kind === "shell") paired.push({ type: "shell", item: pair });
    else paired.push({ type: "tool", item: pair });
  }

  const merged: TimelineBlock[] = [];
  for (const block of paired) {
    if (block.type === "thinking") {
      const previous = merged.at(-1);
      if (previous?.type === "explore") previous.items.push({ kind: "thinking", item: block.item });
      else merged.push(block);
      continue;
    }
    if (block.type === "explore") {
      const leading: ExploreEntry[] = [];
      while (merged.at(-1)?.type === "thinking") {
        const pending = merged.pop();
        if (pending?.type === "thinking") leading.unshift({ kind: "thinking", item: pending.item });
      }
      const previous = merged.at(-1);
      if (previous?.type === "explore") previous.items.push(...leading, ...block.items);
      else merged.push({ type: "explore", items: [...leading, ...block.items] });
      continue;
    }
    merged.push(block);
  }
  return merged;
}
