export type MonacoJsx = "preserve" | "react" | "react-native" | "react-jsx" | "react-jsxdev" | "none";

export type WorkspaceCompilerOptions = {
  allowJs?: boolean;
  allowSyntheticDefaultImports?: boolean;
  esModuleInterop?: boolean;
  experimentalDecorators?: boolean;
  isolatedModules?: boolean;
  jsx?: MonacoJsx;
  module?: string;
  moduleResolution?: string;
  strict?: boolean;
  target?: string;
  useDefineForClassFields?: boolean;
  baseUrl?: string;
  paths?: Record<string, string[]>;
};

export function parseTsconfigJson(text: string): unknown {
  const withoutComments = text
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "");
  return JSON.parse(withoutComments.replace(/,\s*([}\]])/g, "$1"));
}

export function compilerOptionsFromTsconfig(source: unknown): WorkspaceCompilerOptions {
  const root = asRecord(source);
  const options = asRecord(root?.compilerOptions);
  if (!options) return {};
  const jsx = readString(options.jsx);
  const next: WorkspaceCompilerOptions = {};
  const allowJs = readBoolean(options.allowJs);
  const allowSyntheticDefaultImports = readBoolean(options.allowSyntheticDefaultImports);
  const esModuleInterop = readBoolean(options.esModuleInterop);
  const experimentalDecorators = readBoolean(options.experimentalDecorators);
  const isolatedModules = readBoolean(options.isolatedModules);
  const strict = readBoolean(options.strict);
  const useDefineForClassFields = readBoolean(options.useDefineForClassFields);
  if (allowJs != null) next.allowJs = allowJs;
  if (allowSyntheticDefaultImports != null) next.allowSyntheticDefaultImports = allowSyntheticDefaultImports;
  if (esModuleInterop != null) next.esModuleInterop = esModuleInterop;
  if (experimentalDecorators != null) next.experimentalDecorators = experimentalDecorators;
  if (isolatedModules != null) next.isolatedModules = isolatedModules;
  if (jsx && isJsx(jsx)) next.jsx = jsx;
  if (readString(options.module)) next.module = readString(options.module);
  if (readString(options.moduleResolution)) next.moduleResolution = readString(options.moduleResolution);
  if (strict != null) next.strict = strict;
  if (readString(options.target)) next.target = readString(options.target);
  if (useDefineForClassFields != null) next.useDefineForClassFields = useDefineForClassFields;
  if (readString(options.baseUrl)) next.baseUrl = readString(options.baseUrl);
  const paths = readPaths(options.paths);
  if (paths) next.paths = paths;
  return next;
}

/**
 * Monaco 的 TS 只有一份全局配置，而模型路径是 `file:///<workspaceId>/<path>`。
 * 把各工作区的 baseUrl/paths 改写到虚拟根 `file:///` 下合并；同名别名按工作区顺序依次尝试。
 */
export function mergeWorkspaceCompilerOptions(
  entries: Array<{ workspaceId: string; options: WorkspaceCompilerOptions }>,
): WorkspaceCompilerOptions {
  const merged: WorkspaceCompilerOptions = {};
  const paths: Record<string, string[]> = {};
  for (const { workspaceId, options } of entries) {
    const { baseUrl, paths: own, ...rest } = options;
    Object.assign(merged, rest);
    const base = baseUrl ?? (own ? "." : undefined);
    if (base == null) continue;
    const mapping = own ?? { "*": ["*"] };
    for (const [alias, targets] of Object.entries(mapping)) {
      const list = (paths[alias] ??= []);
      for (const target of targets) {
        list.push(joinVirtualPath(workspaceId, base, target));
      }
    }
  }
  if (Object.keys(paths).length) {
    merged.baseUrl = "file:///";
    merged.paths = paths;
  }
  return merged;
}

function joinVirtualPath(workspaceId: string, base: string, target: string): string {
  const parts: string[] = [];
  for (const segment of `${base}/${target}`.replaceAll("\\", "/").split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") parts.pop();
    else parts.push(segment);
  }
  return [workspaceId, ...parts].join("/");
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function readString(value: unknown): string | undefined {
  return typeof value === "string" && value ? value : undefined;
}

function readPaths(value: unknown): Record<string, string[]> | undefined {
  const record = asRecord(value);
  if (!record) return undefined;
  const paths: Record<string, string[]> = {};
  for (const [key, item] of Object.entries(record)) {
    if (Array.isArray(item) && item.every((entry) => typeof entry === "string")) {
      paths[key] = item;
    }
  }
  return Object.keys(paths).length ? paths : undefined;
}

function isJsx(value: string): value is MonacoJsx {
  return ["preserve", "react", "react-native", "react-jsx", "react-jsxdev", "none"].includes(value);
}
