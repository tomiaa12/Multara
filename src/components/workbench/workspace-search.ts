export interface SearchOptions {
  query: string;
  caseSensitive: boolean;
  wholeWord: boolean;
  regex: boolean;
  include: string;
  exclude: string;
  preserveCase: boolean;
  useExcludeSettings: boolean;
}

export interface TextSearchMatch {
  path: string;
  line: number;
  column: number;
  length: number;
  preview: string;
  previewColumn: number;
}

export interface TextSearchResponse {
  matches: TextSearchMatch[];
  truncated: boolean;
  error: string;
}

export const SEARCH_MATCH_LIMIT = 2000;
const PREVIEW_LIMIT = 500;
const SKIP_SEGMENTS = new Set([
  "node_modules",
  ".git",
  "dist",
  "target",
  ".next",
  "coverage",
  "out",
  ".turbo",
  ".cache",
]);

export function emptySearchOptions(): SearchOptions {
  return {
    query: "",
    caseSensitive: false,
    wholeWord: false,
    regex: false,
    include: "",
    exclude: "",
    preserveCase: false,
    useExcludeSettings: true,
  };
}

export function fileIncluded(path: string, options: Pick<SearchOptions, "include" | "exclude" | "useExcludeSettings">): boolean {
  if (options.useExcludeSettings && path.split("/").some((segment) => SKIP_SEGMENTS.has(segment))) return false;
  if (!matchesPatternList(path, options.include, true)) return false;
  if (matchesPatternList(path, options.exclude, false)) return false;
  return true;
}

export function searchDocument(
  path: string,
  content: string,
  options: SearchOptions,
  limit = SEARCH_MATCH_LIMIT,
): TextSearchResponse {
  if (!options.query) return { matches: [], truncated: false, error: "" };
  if (!fileIncluded(path, options)) return { matches: [], truncated: false, error: "" };
  let matcher: RegExp;
  try {
    matcher = buildMatcher(options);
  } catch (error) {
    return { matches: [], truncated: false, error: error instanceof Error ? error.message : "正则无效" };
  }
  const matches: TextSearchMatch[] = [];
  let truncated = false;
  const lines = content.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    matcher.lastIndex = 0;
    let found = matcher.exec(line);
    while (found) {
      if (matches.length >= limit) {
        truncated = true;
        return { matches, truncated, error: "" };
      }
      const column = found.index + 1;
      const length = found[0].length || 1;
      const window = previewWindow(line, found.index, length);
      matches.push({
        path,
        line: index + 1,
        column,
        length,
        preview: window.text,
        previewColumn: window.column,
      });
      if (found[0].length === 0) matcher.lastIndex += 1;
      found = matcher.exec(line);
    }
  }
  return { matches, truncated, error: "" };
}

export function replaceInDocument(content: string, options: SearchOptions, replacement: string): string {
  if (!options.query) return content;
  const matcher = buildMatcher(options);
  return content.replace(matcher, (hit) => applyCase(hit, replacement, options.preserveCase));
}

function applyCase(sample: string, replacement: string, preserve: boolean): string {
  if (!preserve || !sample || !replacement) return replacement;
  if (sample === sample.toUpperCase()) return replacement.toUpperCase();
  if (sample === sample.toLowerCase()) return replacement.toLowerCase();
  if (
    sample[0] === sample[0].toUpperCase()
    && sample.slice(1) === sample.slice(1).toLowerCase()
  ) {
    return replacement[0].toUpperCase() + replacement.slice(1).toLowerCase();
  }
  return replacement;
}

function buildMatcher(options: SearchOptions): RegExp {
  const source = options.regex ? options.query : escapeRegExp(options.query);
  const body = !options.regex && options.wholeWord ? `\\b(?:${source})\\b` : source;
  const flags = options.caseSensitive ? "g" : "gi";
  return new RegExp(body, flags);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function previewWindow(line: string, start: number, length: number): { text: string; column: number } {
  if (line.length <= PREVIEW_LIMIT) return { text: line, column: start + 1 };
  const from = Math.max(0, start - 80);
  const text = `${from > 0 ? "…" : ""}${line.slice(from, from + PREVIEW_LIMIT)}`;
  return { text, column: start - from + (from > 0 ? 2 : 1) };
}

function matchesPatternList(path: string, raw: string, emptyMatches: boolean): boolean {
  const patterns = raw.split(",").map((item) => item.trim()).filter(Boolean);
  if (!patterns.length) return emptyMatches;
  return patterns.some((pattern) => globMatch(path, pattern));
}

export function globMatch(path: string, pattern: string): boolean {
  const normalized = pattern.trim().replace(/\\/g, "/").replace(/^\.\//, "").replace(/^\/+|\/+$/g, "");
  if (!normalized) return false;
  // 没有通配符时按文件夹前缀匹配，和 VS Code「在文件夹中查找」填的 ./src/components 一样。
  if (!/[*?]/.test(normalized)) {
    return path === normalized || path.startsWith(`${normalized}/`);
  }
  const target = normalized.includes("/") ? path : path.split("/").pop() ?? path;
  return globToRegExp(normalized).test(target);
}

function globToRegExp(pattern: string): RegExp {
  let source = "";
  let index = 0;
  while (index < pattern.length) {
    if (pattern.startsWith("**/", index)) {
      source += "(?:^|.*/)";
      index += 3;
      continue;
    }
    if (pattern.startsWith("**", index)) {
      source += ".*";
      index += 2;
      continue;
    }
    const char = pattern[index];
    if (char === "*") source += "[^/]*";
    else if (char === "?") source += "[^/]";
    else source += char.replace(/[.+^${}()|[\]\\]/g, "\\$&");
    index += 1;
  }
  return new RegExp(`^${source}$`);
}
