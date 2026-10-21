export type ExplorerSortOrder = "default" | "mixed" | "filesFirst" | "type" | "modified" | "foldersNestsFiles";
export type ExplorerLexicographic = "default" | "upper" | "lower" | "unicode";
export type ExplorerIncrementalNaming = "simple" | "smart" | "disabled";
export type ExplorerPathSeparator = "/" | "\\" | "auto";
export type ExplorerConfirmUndo = "verbose" | "default" | "light";
export type ExplorerExcludeValue = boolean | { when: string };

/** 与 VS Code `files.contribution.ts` 中 explorer.* / files.exclude 的默认值保持一致。 */
export interface ExplorerSettings {
  sortOrder: ExplorerSortOrder;
  sortOrderLexicographicOptions: ExplorerLexicographic;
  sortOrderReverse: boolean;
  compactFolders: boolean;
  autoReveal: boolean | "focusNoScroll";
  autoRevealExclude: Record<string, ExplorerExcludeValue>;
  enableDragAndDrop: boolean;
  confirmDragAndDrop: boolean;
  confirmPasteNative: boolean;
  confirmDelete: boolean;
  enableUndo: boolean;
  confirmUndo: ExplorerConfirmUndo;
  incrementalNaming: ExplorerIncrementalNaming;
  autoOpenDroppedFile: boolean;
  copyRelativePathSeparator: ExplorerPathSeparator;
  copyPathSeparator: ExplorerPathSeparator;
  excludeGitIgnore: boolean;
  decorationsColors: boolean;
  decorationsBadges: boolean;
  fileNestingEnabled: boolean;
  fileNestingExpand: boolean;
  fileNestingPatterns: Record<string, string>;
  filesExclude: Record<string, ExplorerExcludeValue>;
}

export const DEFAULT_EXPLORER_SETTINGS: ExplorerSettings = {
  sortOrder: "default",
  sortOrderLexicographicOptions: "default",
  sortOrderReverse: false,
  compactFolders: true,
  autoReveal: true,
  autoRevealExclude: { "**/node_modules": true, "**/bower_components": true },
  enableDragAndDrop: true,
  confirmDragAndDrop: true,
  confirmPasteNative: true,
  confirmDelete: true,
  enableUndo: true,
  confirmUndo: "default",
  incrementalNaming: "simple",
  autoOpenDroppedFile: true,
  copyRelativePathSeparator: "auto",
  copyPathSeparator: "auto",
  excludeGitIgnore: false,
  decorationsColors: true,
  decorationsBadges: true,
  fileNestingEnabled: false,
  fileNestingExpand: true,
  fileNestingPatterns: {
    "*.ts": "${capture}.js",
    "*.js": "${capture}.js.map, ${capture}.min.js, ${capture}.d.ts",
    "*.jsx": "${capture}.js",
    "*.tsx": "${capture}.ts",
    "tsconfig.json": "tsconfig.*.json",
    "package.json": "package-lock.json, yarn.lock, pnpm-lock.yaml, bun.lockb, bun.lock",
  },
  filesExclude: {
    "**/.git": true,
    "**/.svn": true,
    "**/.hg": true,
    "**/.jj": true,
    "**/CVS": true,
    "**/.DS_Store": true,
    "**/Thumbs.db": true,
  },
};

const ENUMS: Partial<Record<keyof ExplorerSettings, readonly unknown[]>> = {
  sortOrder: ["default", "mixed", "filesFirst", "type", "modified", "foldersNestsFiles"],
  sortOrderLexicographicOptions: ["default", "upper", "lower", "unicode"],
  autoReveal: [true, false, "focusNoScroll"],
  confirmUndo: ["verbose", "default", "light"],
  incrementalNaming: ["simple", "smart", "disabled"],
  copyRelativePathSeparator: ["/", "\\", "auto"],
  copyPathSeparator: ["/", "\\", "auto"],
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function excludeMap(value: unknown, fallback: Record<string, ExplorerExcludeValue>) {
  if (!isRecord(value)) return fallback;
  const out: Record<string, ExplorerExcludeValue> = {};
  for (const [pattern, entry] of Object.entries(value)) {
    if (typeof entry === "boolean") out[pattern] = entry;
    else if (isRecord(entry) && typeof entry.when === "string") out[pattern] = { when: entry.when };
  }
  return out;
}

/** 解析设置里保存的 JSON；未知或类型不对的字段回落到默认值。 */
export function parseExplorerSettings(raw: string | undefined | null): ExplorerSettings {
  let parsed: unknown = null;
  try {
    parsed = raw ? JSON.parse(raw) : null;
  } catch {
    parsed = null;
  }
  const result: ExplorerSettings = structuredClone(DEFAULT_EXPLORER_SETTINGS);
  if (!isRecord(parsed)) return result;
  for (const key of Object.keys(DEFAULT_EXPLORER_SETTINGS) as Array<keyof ExplorerSettings>) {
    const value = parsed[key];
    if (value === undefined) continue;
    const allowed = ENUMS[key];
    if (allowed) {
      if (allowed.includes(value)) (result as unknown as Record<string, unknown>)[key] = value;
      continue;
    }
    const fallback = DEFAULT_EXPLORER_SETTINGS[key];
    if (key === "filesExclude" || key === "autoRevealExclude") {
      result[key] = excludeMap(value, result[key]);
    } else if (key === "fileNestingPatterns") {
      if (isRecord(value)) {
        result.fileNestingPatterns = Object.fromEntries(
          Object.entries(value).filter((entry): entry is [string, string] => typeof entry[1] === "string"),
        );
      }
    } else if (typeof value === typeof fallback) {
      (result as unknown as Record<string, unknown>)[key] = value;
    }
  }
  return result;
}

/** 只保存与默认值不同的字段，方便以后调整默认值。 */
export function serializeExplorerSettings(settings: ExplorerSettings): string {
  const diff: Record<string, unknown> = {};
  for (const key of Object.keys(DEFAULT_EXPLORER_SETTINGS) as Array<keyof ExplorerSettings>) {
    if (JSON.stringify(settings[key]) !== JSON.stringify(DEFAULT_EXPLORER_SETTINGS[key])) {
      diff[key] = settings[key];
    }
  }
  return JSON.stringify(diff);
}

export function applySeparator(path: string, separator: ExplorerPathSeparator, isWindows: boolean): string {
  const useBackslash = separator === "\\" || (separator === "auto" && isWindows);
  return useBackslash ? path.replace(/\//g, "\\") : path.replace(/\\/g, "/");
}

/** 设置页用：一行一个 glob，只展示值为 true 的规则。 */
export function filesExcludeToText(map: Record<string, ExplorerExcludeValue>): string {
  return Object.entries(map)
    .filter(([, value]) => value === true)
    .map(([pattern]) => pattern)
    .join("\n");
}

export function filesExcludeFromText(text: string): Record<string, ExplorerExcludeValue> {
  const out: Record<string, ExplorerExcludeValue> = {};
  for (const line of text.split(/\r?\n/)) {
    const pattern = line.trim();
    if (!pattern || pattern.startsWith("#")) continue;
    out[pattern] = true;
  }
  return out;
}
