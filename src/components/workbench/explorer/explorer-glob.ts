import type { ExplorerExcludeValue } from "./explorer-settings";

/**
 * VS Code glob 语法（`files.exclude` 用）：`*`、`?`、`**`、`{a,b}`、`[abc]`、`[!abc]`。
 * 路径一律用 `/` 分隔，并相对工作区根目录匹配。
 */
function splitAlternatives(body: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const char of body) {
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (char === "," && depth === 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += char;
  }
  parts.push(current);
  return parts;
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&");
}

export function globToRegExpSource(pattern: string): string {
  let out = "";
  for (let index = 0; index < pattern.length; index += 1) {
    const char = pattern[index];
    if (char === "*") {
      if (pattern[index + 1] === "*") {
        const atSegmentStart = index === 0 || pattern[index - 1] === "/";
        const next = pattern[index + 2];
        index += 1;
        if (atSegmentStart && next === "/") {
          index += 1;
          out += "(?:.*/)?";
        } else if (atSegmentStart && next === undefined) {
          out += ".*";
        } else {
          out += "[^/]*";
        }
      } else {
        out += "[^/]*";
      }
      continue;
    }
    if (char === "?") {
      out += "[^/]";
      continue;
    }
    if (char === "{") {
      let depth = 1;
      let end = index + 1;
      for (; end < pattern.length && depth > 0; end += 1) {
        if (pattern[end] === "{") depth += 1;
        if (pattern[end] === "}") depth -= 1;
      }
      if (depth !== 0) {
        out += escapeRegExp(char);
        continue;
      }
      const body = pattern.slice(index + 1, end - 1);
      out += `(?:${splitAlternatives(body).map(globToRegExpSource).join("|")})`;
      index = end - 1;
      continue;
    }
    if (char === "[") {
      const end = pattern.indexOf("]", index + 1);
      if (end === -1) {
        out += escapeRegExp(char);
        continue;
      }
      let body = pattern.slice(index + 1, end);
      const negate = body.startsWith("!") || body.startsWith("^");
      if (negate) body = body.slice(1);
      out += `[${negate ? "^" : ""}${body.replace(/[\\\]]/g, "\\$&")}]`;
      index = end;
      continue;
    }
    out += escapeRegExp(char);
  }
  return out;
}

const compiled = new Map<string, RegExp>();

export function globMatch(pattern: string, path: string): boolean {
  let regex = compiled.get(pattern);
  if (!regex) {
    const normalized = pattern.replace(/\\/g, "/").replace(/^\.\//, "").replace(/\/+$/, "");
    regex = new RegExp(`^${globToRegExpSource(normalized)}$`);
    if (compiled.size > 500) compiled.clear();
    compiled.set(pattern, regex);
  }
  return regex.test(path);
}

function basenameWithoutExtension(name: string) {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/**
 * `files.exclude` 的判断：值为 true 时直接排除；`{ when: "$(basename).ts" }`
 * 只有在同目录存在对应兄弟文件时才排除。
 */
export function isExcluded(
  relativePath: string,
  name: string,
  expression: Record<string, ExplorerExcludeValue>,
  hasSibling: (name: string) => boolean,
): boolean {
  for (const [pattern, value] of Object.entries(expression)) {
    if (!value) continue;
    if (!globMatch(pattern, relativePath)) continue;
    if (value === true) return true;
    const sibling = value.when.replace(/\$\(basename\)/g, basenameWithoutExtension(name));
    if (hasSibling(sibling)) return true;
  }
  return false;
}
