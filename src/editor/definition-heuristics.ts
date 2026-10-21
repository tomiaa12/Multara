/** 没有语言服务器时的兜底跳转：import 路径、Vue 组件标签、按关键字匹配的定义行。 */

export const SCRIPT_EXTENSIONS = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

const IMPORT_EXTENSIONS = [
  ...SCRIPT_EXTENSIONS,
  "vue",
  "svelte",
  "json",
  "css",
  "scss",
  "sass",
  "less",
  "styl",
];

const LANGUAGE_FAMILIES = [
  [...SCRIPT_EXTENSIONS, "vue", "svelte"],
  ["rs"],
  ["py", "pyi"],
  ["go"],
  ["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"],
  ["java", "kt", "kts"],
  ["cs"],
  ["php"],
  ["rb"],
  ["swift"],
  ["dart"],
  ["lua"],
];

const DEFINITION_KEYWORDS = [
  "function\\*?",
  "func",
  "fn",
  "def",
  "class",
  "struct",
  "enum",
  "union",
  "trait",
  "interface",
  "type",
  "typedef",
  "const",
  "let",
  "var",
  "val",
  "mod",
  "module",
  "namespace",
  "macro_rules!",
  "record",
  "object",
  "protocol",
];

export function extensionOf(path: string): string {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function isScriptPath(path: string): boolean {
  return SCRIPT_EXTENSIONS.includes(extensionOf(path));
}

export function normalizeDriveLetter(path: string): string {
  const slashed = path.replaceAll("\\", "/");
  return /^[a-z]:\//.test(slashed) ? slashed[0].toUpperCase() + slashed.slice(1) : slashed;
}

export function escapeRegex(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** 同时用于 JS RegExp 和后端 Rust regex，只能用两边都支持的语法。 */
export function definitionPattern(word: string): string {
  const name = escapeRegex(word);
  return [
    `\\b(?:${DEFINITION_KEYWORDS.join("|")})\\s+${name}\\b`,
    `\\bfunc\\s*\\([^)]*\\)\\s*${name}\\b`,
    `#define\\s+${name}\\b`,
  ].join("|");
}

export type LineMatch = { line: number; column: number };

/** 返回定义名本身的位置（行列都从 1 开始）。所有分支都以名字结尾，所以名字在匹配末尾。 */
export function findDefinitionLines(lines: string[], word: string): LineMatch[] {
  const pattern = new RegExp(definitionPattern(word), "g");
  const found: LineMatch[] = [];
  lines.forEach((text, index) => {
    for (const match of text.matchAll(pattern)) {
      found.push({ line: index + 1, column: (match.index ?? 0) + match[0].length - word.length + 1 });
    }
  });
  return found;
}

export function definitionColumn(matchColumn: number, matchLength: number, word: string): number {
  return matchColumn + matchLength - word.length;
}

export function searchIncludeFor(path: string): string {
  const extension = extensionOf(path);
  const family = LANGUAGE_FAMILIES.find((items) => items.includes(extension));
  if (family) return family.map((item) => `*.${item}`).join(",");
  return extension ? `*.${extension}` : "";
}

/** 光标所在的字符串字面量内容；`column` 从 1 开始。 */
export function stringLiteralAt(line: string, column: number): string | null {
  const index = column - 1;
  for (const match of line.matchAll(/(["'`])((?:\\.|(?!\1)[^\\])*)\1/g)) {
    const start = match.index ?? 0;
    const end = start + match[0].length;
    if (index > start && index < end - 1) return match[2];
  }
  return null;
}

export function dirnameOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index < 0 ? "" : path.slice(0, index);
}

export function normalizeRelativePath(path: string): string | null {
  const parts: string[] = [];
  for (const segment of path.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else {
      parts.push(segment);
    }
  }
  return parts.join("/");
}

/** 把 import/require/@import 里的路径解析到工作区内真实存在的文件。 */
export function resolveImportSpecifier(
  specifier: string,
  fromPath: string,
  exists: (path: string) => boolean,
): string | null {
  const spec = specifier.split(/[?#]/)[0];
  let base: string | null;
  if (spec.startsWith("./") || spec.startsWith("../")) {
    base = normalizeRelativePath(`${dirnameOf(fromPath)}/${spec}`);
  } else if (spec.startsWith("@/") || spec.startsWith("~/")) {
    base = normalizeRelativePath(`src/${spec.slice(2)}`);
  } else if (spec.startsWith("/")) {
    base = normalizeRelativePath(spec);
  } else {
    return null;
  }
  if (!base) return null;
  const candidates = [
    base,
    ...IMPORT_EXTENSIONS.map((extension) => `${base}.${extension}`),
    ...IMPORT_EXTENSIONS.map((extension) => `${base}/index.${extension}`),
  ];
  return candidates.find(exists) ?? null;
}

/** `my-button` / `MyButton` 都可能对应 `MyButton.vue`；全小写无连字符的是原生标签（`button`），不算组件。 */
export function componentFileNames(word: string): string[] {
  if (!/^[A-Za-z][\w-]*$/.test(word)) return [];
  if (!word.includes("-") && !/[A-Z]/.test(word)) return [];
  const pascal = word
    .split("-")
    .filter(Boolean)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join("");
  const kebab = pascal.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
  return [...new Set([`${pascal}.vue`, `${kebab}.vue`])];
}
