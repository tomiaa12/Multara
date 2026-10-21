export interface TerminalInputToken {
  kind: "command" | "path";
  token: string;
  directory: string;
  name: string;
}

export type TerminalSuggestionKind = "command" | "file" | "folder";

export interface TerminalSuggestion {
  name: string;
  kind: TerminalSuggestionKind;
}

/** 没有 shell 集成时用来猜提示符位置的标记。 */
const PROMPT_MARKS = ["% ", "$ ", "# ", "❯ ", "➜ ", "> "];
const MAX_SUGGESTIONS = 10;

function promptEnd(text: string) {
  let marker = -1;
  let markLength = 0;
  for (const mark of PROMPT_MARKS) {
    const index = text.lastIndexOf(mark);
    if (index > marker) {
      marker = index;
      markLength = mark.length;
    }
  }
  // cmd 的提示符 `C:\dir>` 后面没有空格
  const cmd = text.match(/^[A-Za-z]:\\[^>]*>/);
  if (cmd && cmd[0].length > marker + markLength) return cmd[0].length;
  return marker < 0 ? -1 : marker + markLength;
}

/**
 * 从光标所在行取出当前输入。有 shell 集成时 `inputStart` 是输入起点列；
 * 否则按提示符标记猜，没有提示符时不猜测，避免在命令输出上弹出补全。
 */
export function terminalInputToken(line: string, cursor: number, inputStart?: number | null): TerminalInputToken | null {
  const text = line.slice(0, Math.max(0, Math.min(cursor, line.length)));
  const start = inputStart ?? promptEnd(text);
  if (start < 0 || start > text.length) return null;
  const input = text.slice(start);
  const trimmed = input.replace(/\s+$/, "");
  if (!trimmed || trimmed !== input) return null;
  const parts = trimmed.trimStart().split(/\s+/);
  const token = parts[parts.length - 1] ?? "";
  if (!token) return null;
  const slash = Math.max(token.lastIndexOf("/"), token.lastIndexOf("\\"));
  const directory = slash >= 0 ? token.slice(0, slash + 1) : "";
  const name = slash >= 0 ? token.slice(slash + 1) : token;
  return {
    kind: parts.length === 1 && slash < 0 ? "command" : "path",
    token,
    directory,
    name,
  };
}

export function isAbsoluteTerminalPath(path: string) {
  return path.startsWith("/") || path.startsWith("\\\\") || /^[A-Za-z]:[\\/]/.test(path);
}

/** 补全要列出的目录。`~` 交给后端按用户目录解析。 */
export function terminalCompletionDirectory(cwd: string, directory: string, windows: boolean) {
  if (!directory) return cwd;
  if (isAbsoluteTerminalPath(directory) || directory.startsWith("~")) return directory;
  const separator = windows ? "\\" : "/";
  const base = cwd.replace(/[\\/]+$/, "");
  return `${base}${separator}${windows ? directory.replaceAll("/", "\\") : directory}`;
}

/** 先列前缀匹配，再列包含匹配；同类按名字排序。 */
export function rankTerminalSuggestions<T extends string | TerminalSuggestion>(candidates: T[], prefix: string): T[] {
  const query = prefix.toLowerCase();
  if (!query) return [];
  const nameOf = (item: T) => (typeof item === "string" ? item : item.name);
  const starts: T[] = [];
  const contains: T[] = [];
  for (const item of candidates) {
    const name = nameOf(item).toLowerCase();
    if (name === query) continue;
    if (name.startsWith(query)) starts.push(item);
    else if (query.length >= 2 && name.includes(query)) contains.push(item);
  }
  const byName = (a: T, b: T) => nameOf(a).localeCompare(nameOf(b));
  return [...starts.sort(byName), ...contains.sort(byName)].slice(0, MAX_SUGGESTIONS);
}

/** 接受补全时要发给 shell 的文本：退格删掉已输入的部分，再写入完整名字。 */
export function suggestionInsertText(typed: string, pick: TerminalSuggestion, windows: boolean) {
  const suffix = pick.kind === "folder" ? (windows ? "\\" : "/") : "";
  const full = pick.name + suffix;
  if (full.toLowerCase().startsWith(typed.toLowerCase()) && full.startsWith(typed)) {
    return full.slice(typed.length);
  }
  return "\x7f".repeat(typed.length) + full;
}
