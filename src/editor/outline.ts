export type OutlineItem = {
  label: string;
  line: number;
  endLine: number;
  kind: "class" | "function" | "heading" | "property" | "type";
};

/** 超过这个规模的文件不算大纲，避免打字时主线程卡住。 */
export const OUTLINE_MAX_CHARS = 2 * 1024 * 1024;
export const OUTLINE_MAX_LINES = 50_000;

export function outlineFromContent(language: string, content: string): OutlineItem[] {
  if (content.length > OUTLINE_MAX_CHARS) return [];
  const lines = content.split(/\r?\n/);
  if (lines.length > OUTLINE_MAX_LINES) return [];
  const items = rawOutline(language, lines);
  return items.map((item, index) => ({
    ...item,
    endLine: symbolEndLine(lines, items, index),
  }));
}

export function enclosingOutline(items: OutlineItem[], line: number): OutlineItem | null {
  let current: OutlineItem | null = null;
  for (const item of items) {
    if (item.line <= line && line <= item.endLine) current = item;
  }
  return current;
}

export function fileBreadcrumbs(path: string): string[] {
  return path.replace(/\\/g, "/").split("/").filter(Boolean);
}

type OutlineDraft = Omit<OutlineItem, "endLine">;

function markdownOutline(lines: string[]): OutlineDraft[] {
  return linesOf(lines).flatMap(([line, index]) => {
    const match = line.match(/^(#{1,6})\s+(.+?)\s*$/);
    return match ? [{ label: match[2], line: index, kind: "heading" as const }] : [];
  });
}

function jsonOutline(lines: string[]): OutlineDraft[] {
  return linesOf(lines).flatMap(([line, index]) => {
    const match = line.match(/^(\s*)"([^"]+)"\s*:/);
    return match && match[1].length <= 2
      ? [{ label: match[2], line: index, kind: "property" as const }]
      : [];
  });
}

function yamlOutline(lines: string[]): OutlineDraft[] {
  return linesOf(lines).flatMap(([line, index]) => {
    const match = line.match(/^([A-Za-z0-9_.-]+)\s*:/);
    return match ? [{ label: match[1], line: index, kind: "property" as const }] : [];
  });
}

function htmlOutline(lines: string[]): OutlineDraft[] {
  const items: OutlineDraft[] = [];
  for (const [line, index] of linesOf(lines)) {
    const heading = line.match(/<h[1-6][^>]*>([^<]+)/i);
    const id = line.match(/<(section|article|div|main)[^>]*id=["']([^"']+)/i);
    if (heading) items.push({ label: heading[1].trim(), line: index, kind: "heading" });
    else if (id) items.push({ label: id[2], line: index, kind: "property" });
  }
  return items;
}

function codeOutline(language: string, lines: string[]): OutlineDraft[] {
  const patterns: Array<{ kind: OutlineItem["kind"]; re: RegExp }> = [
    { kind: "class", re: /^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_]\w*)/ },
    { kind: "type", re: /^\s*(?:export\s+)?(?:interface|type|enum)\s+([A-Za-z_]\w*)/ },
    { kind: "function", re: /^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_]\w*)/ },
    { kind: "function", re: /^\s*(?:export\s+)?const\s+([A-Za-z_]\w*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_]\w*)\s*=>/ },
    { kind: "function", re: /^\s*(?:pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)/ },
    { kind: "type", re: /^\s*(?:pub(?:\(crate\))?\s+)?(?:struct|enum|trait|impl(?:\s*<[^>]+>)?)\s+([A-Za-z_]\w*)/ },
    { kind: "function", re: /^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)/ },
    { kind: "class", re: /^\s*class\s+([A-Za-z_]\w*)/ },
    { kind: "function", re: /^\s*func\s+(?:\([^)]*\)\s+)?([A-Za-z_]\w*)/ },
    { kind: "type", re: /^\s*type\s+([A-Za-z_]\w*)/ },
  ];
  if (language === "css" || language === "scss" || language === "less") {
    return linesOf(lines).flatMap(([line, index]) => {
      const match = line.match(/^([.#]?[\w-]+(?:\s*,\s*[.#]?[\w-]+)*)\s*\{/);
      return match ? [{ label: match[1], line: index, kind: "property" as const }] : [];
    });
  }
  return linesOf(lines).flatMap(([line, index]) => {
    for (const pattern of patterns) {
      const match = line.match(pattern.re);
      if (match) return [{ label: match[1], line: index, kind: pattern.kind }];
    }
    return [];
  });
}

function rawOutline(language: string, lines: string[]): Array<Omit<OutlineItem, "endLine">> {
  if (language === "markdown" || language === "mdx") return markdownOutline(lines);
  if (language === "json") return jsonOutline(lines);
  if (language === "yaml") return yamlOutline(lines);
  if (language === "html" || language === "vue" || language === "xml") return htmlOutline(lines);
  return codeOutline(language, lines);
}

/** 符号只覆盖到下一个同级或更外层符号之前。光标离开这段范围后，面包屑不再显示它。 */
function symbolEndLine(
  lines: string[],
  items: OutlineDraft[],
  index: number,
): number {
  const item = items[index];
  if (!item) return 1;
  if (item.kind === "heading") return headingEndLine(lines, item.line);
  return codeEndLine(lines, items, index);
}

function headingEndLine(lines: string[], start: number) {
  const startLine = lines[start - 1] ?? "";
  const markdown = startLine.match(/^(#{1,6})\s/);
  if (markdown) {
    const level = markdown[1].length;
    for (let line = start + 1; line <= lines.length; line += 1) {
      const next = (lines[line - 1] ?? "").match(/^(#{1,6})\s/);
      if (next && next[1].length <= level) return line - 1;
    }
    return lines.length;
  }
  const html = startLine.match(/<h([1-6])/i);
  if (!html) return start;
  const close = new RegExp(`</h${html[1]}>`, "i");
  for (let line = start; line <= lines.length; line += 1) {
    if (close.test(lines[line - 1] ?? "")) return line;
  }
  return start;
}

function codeEndLine(lines: string[], items: OutlineDraft[], index: number) {
  const start = items[index]?.line ?? 1;
  const next = items[index + 1]?.line;
  const startIndent = indentOf(lines[start - 1] ?? "");
  let end = lines.length;
  for (let line = start + 1; line <= lines.length; line += 1) {
    if (next && line >= next) {
      end = next - 1;
      break;
    }
    const text = lines[line - 1] ?? "";
    if (!text.trim()) continue;
    if (indentOf(text) <= startIndent && !isClosingBrace(text)) {
      end = line - 1;
      break;
    }
    if (isClosingBrace(text) && indentOf(text) <= startIndent) {
      end = line;
      break;
    }
  }
  return Math.max(start, end);
}

function indentOf(line: string) {
  return line.match(/^\s*/)?.[0].length ?? 0;
}

function isClosingBrace(line: string) {
  return /^[\s)]*[}\]]/.test(line);
}

function linesOf(lines: string[]): Array<[string, number]> {
  return lines.map((line, index) => [line, index + 1]);
}
