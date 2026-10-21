export const jsoncDiagnosticsOptions = {
  validate: true,
  allowComments: true,
  comments: "ignore" as const,
  trailingCommas: "ignore" as const,
};

export const AUTO_DETECT_LANGUAGE = "auto";

export type LanguageDescriptor = {
  id: string;
  aliases?: string[] | null;
  extensions?: string[] | null;
  filenames?: string[] | null;
};

export type EditorLanguageOption = {
  id: string;
  label: string;
  aliases?: string[];
  extensions?: string[];
  filenames?: string[];
};

const LANGUAGE_ICON_FILES: Record<string, string> = {
  bat: "file.bat",
  c: "file.c",
  cpp: "file.cpp",
  csharp: "file.cs",
  css: "file.css",
  dockerfile: "Dockerfile",
  go: "file.go",
  html: "file.html",
  ignore: ".gitignore",
  ini: "file.ini",
  java: "file.java",
  javascript: "file.js",
  json: "file.json",
  jsonc: "tsconfig.json",
  kotlin: "file.kt",
  less: "file.less",
  markdown: "file.md",
  php: "file.php",
  plaintext: "file.txt",
  powershell: "file.ps1",
  python: "file.py",
  ruby: "file.rb",
  rust: "file.rs",
  scss: "file.scss",
  shell: "file.sh",
  sql: "file.sql",
  swift: "file.swift",
  typescript: "file.ts",
  xml: "file.xml",
  yaml: "file.yaml",
  yml: "file.yml",
};

export function languageIconName(
  option: Pick<EditorLanguageOption, "id" | "extensions" | "filenames">,
): string {
  if (option.id === AUTO_DETECT_LANGUAGE) return "file";
  const filename = option.filenames?.[0];
  if (filename) return filename;
  const ext = option.extensions?.[0];
  if (ext) return ext.startsWith(".") ? `file${ext}` : `file.${ext}`;
  return LANGUAGE_ICON_FILES[option.id] ?? "file";
}

export function resolveMonacoLanguageId(
  language: string,
  isRegistered: (id: string) => boolean,
): string {
  const id = language === "jsonc" ? "json" : language;
  return isRegistered(id) ? id : "plaintext";
}

export function shouldSuppressEmbeddedDiagnostics(languageId: string, path = ""): boolean {
  // .vue 按 HTML 高亮，但没有 Vue 语言服务，校验会把整篇标成波浪线。
  if (path.toLowerCase().endsWith(".vue")) return true;
  return languageId === "markdown" || languageId === "mdx";
}

export function editorLanguageKey(workspaceId: string, path: string): string {
  return `${workspaceId}:${path}`;
}

export function applyLanguageOverride<T extends { workspaceId: string; path: string; language: string }>(
  file: T,
  overrides: Record<string, string>,
): T {
  const language = overrides[editorLanguageKey(file.workspaceId, file.path)];
  return language ? { ...file, language } : file;
}

export function languageDisplayName(
  id: string,
  languages: Array<LanguageDescriptor | EditorLanguageOption>,
): string {
  if (id === AUTO_DETECT_LANGUAGE) return "自动检测";
  const match = languages.find((item) => item.id === id);
  if (!match) return id;
  if ("label" in match && match.label) return match.label;
  if ("aliases" in match && match.aliases?.[0]) return match.aliases[0];
  return id;
}

export function listSelectableLanguages(languages: LanguageDescriptor[]): EditorLanguageOption[] {
  const seen = new Set<string>();
  return languages
    .filter((item) => item.id && !seen.has(item.id) && seen.add(item.id))
    .map((item) => ({
      id: item.id,
      label: item.aliases?.[0] || item.id,
      aliases: item.aliases ?? undefined,
      extensions: item.extensions ?? undefined,
      filenames: item.filenames ?? undefined,
    }))
    .sort((a, b) => a.label.localeCompare(b.label, "en", { sensitivity: "base" }));
}

export function filterSelectableLanguages(
  languages: EditorLanguageOption[],
  query: string,
): EditorLanguageOption[] {
  const q = query.trim().toLowerCase();
  if (!q) return languages;
  return languages.filter(
    (item) =>
      item.label.toLowerCase().includes(q) ||
      item.id.toLowerCase().includes(q) ||
      item.aliases?.some((alias) => alias.toLowerCase().includes(q)),
  );
}

export function matchesAutoDetect(query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return "自动检测".includes(q) || "auto detect".includes(q) || "autodetect".includes(q);
}
