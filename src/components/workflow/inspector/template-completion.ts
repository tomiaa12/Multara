/** 模板补全的纯函数：变量树拍平、光标处 token 识别、应用补全。 */

export interface CompletionItem {
  /** 点号路径，如 nodes.prd.outputs.spec */
  path: string;
  /** 值类型提示 */
  kind: "object" | "array" | "string" | "number" | "boolean" | "null";
}

const MAX_DEPTH = 6;
const MAX_ITEMS = 400;

function kindOf(value: unknown): CompletionItem["kind"] {
  if (value === null || value === undefined) return "null";
  if (Array.isArray(value)) return "array";
  if (typeof value === "object") return "object";
  if (typeof value === "number") return "number";
  if (typeof value === "boolean") return "boolean";
  return "string";
}

/** 把变量树拍平为 `a.b.c` 路径列表（数组只展开到数组本身） */
export function flattenVariables(tree: unknown, prefix = "", depth = 0, out: CompletionItem[] = []): CompletionItem[] {
  if (!tree || typeof tree !== "object" || Array.isArray(tree) || depth >= MAX_DEPTH) return out;
  for (const [key, value] of Object.entries(tree as Record<string, unknown>)) {
    if (out.length >= MAX_ITEMS) break;
    const path = prefix ? `${prefix}.${key}` : key;
    out.push({ path, kind: kindOf(value) });
    flattenVariables(value, path, depth + 1, out);
  }
  return out;
}

export interface CompletionToken {
  /** token 在文本中的起止位置 */
  start: number;
  end: number;
  /** 已输入的前缀 */
  query: string;
}

/**
 * 光标是否处在 `{{ ... }}` / `{% ... %}` 内（表达式模式下整段都算）。
 * 返回光标前正在输入的标识符 token（可含点号）。
 */
export function tokenAtCursor(text: string, cursor: number, expression = false): CompletionToken | null {
  const before = text.slice(0, cursor);
  if (!expression) {
    const open = Math.max(before.lastIndexOf("{{"), before.lastIndexOf("{%"));
    if (open < 0) return null;
    const close = Math.max(before.lastIndexOf("}}"), before.lastIndexOf("%}"));
    if (close > open) return null;
  }
  const match = /[A-Za-z_$][\w$]*(?:\.[\w$]*)*\.?$/.exec(before);
  if (!match) {
    return expression || /(\{\{|\{%)\s*$/.test(before) ? { start: cursor, end: cursor, query: "" } : null;
  }
  return { start: match.index, end: cursor, query: match[0] };
}

export function filterCompletions(items: CompletionItem[], query: string, limit = 12): CompletionItem[] {
  const needle = query.toLowerCase();
  const starts: CompletionItem[] = [];
  const contains: CompletionItem[] = [];
  for (const item of items) {
    const lower = item.path.toLowerCase();
    if (needle && lower === needle) continue;
    if (!needle || lower.startsWith(needle)) starts.push(item);
    else if (lower.includes(needle)) contains.push(item);
  }
  return [...starts, ...contains].slice(0, limit);
}

export function applyCompletion(text: string, token: CompletionToken, path: string, expression = false): { text: string; cursor: number } {
  let tail = text.slice(token.end);
  let insert = path;
  // 在模板里补全后顺手补上结束符
  if (!expression && !/^\s*(\}\}|%\})/.test(tail)) {
    insert += " }}";
    tail = tail.replace(/^\s*/, "");
  }
  const next = text.slice(0, token.start) + insert + tail;
  return { text: next, cursor: token.start + path.length };
}
