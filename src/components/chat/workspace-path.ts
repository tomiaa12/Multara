export interface PathSegment {
  type: "text" | "path";
  text: string;
  path?: string;
  startLine?: number;
  endLine?: number;
}

export interface CodeFenceInfo {
  language: string;
  path?: string;
  startLine?: number;
  endLine?: number;
}

const PATH_PATTERN = /((?:[A-Za-z0-9_.@+-]+\/)+[A-Za-z0-9_.@+-]+\.[A-Za-z0-9]+)(?::(\d+)(?:-(\d+))?)?/g;
const SOURCE_EXT =
  /\.(?:[jt]sx?|mjs|cjs|vue|svelte|rs|go|py|md|css|scss|less|json|toml|ya?ml|html?|svg|sh|bash|zsh|sql|graphql|proto|swift|kt|java|rb|php|cs|cpp|cxx|h|hpp)$/i;

function looksLikeFilePath(value: string): boolean {
  const path = value.trim();
  if (!path || /\s/.test(path)) return false;
  if (path.includes("/")) return SOURCE_EXT.test(path) || /\.[A-Za-z0-9]+$/.test(path);
  return SOURCE_EXT.test(path);
}

function languageFromPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  if (ext === "ts" || ext === "tsx" || ext === "mts" || ext === "cts") return "typescript";
  if (ext === "js" || ext === "jsx" || ext === "mjs" || ext === "cjs") return "javascript";
  if (ext === "vue" || ext === "svelte" || ext === "html" || ext === "htm" || ext === "svg") return "xml";
  if (ext === "sh" || ext === "zsh" || ext === "bash") return "bash";
  if (ext === "yml") return "yaml";
  if (ext === "md") return "markdown";
  if (ext === "rs") return "rust";
  if (ext === "py") return "python";
  return ext;
}

/** 解析围栏 info：`12:40:src/App.vue` / `ts:12:40:src/App.vue` / `src/App.vue` / `typescript`。 */
export function parseCodeFenceInfo(raw: string): CodeFenceInfo {
  const info = raw.trim();
  if (!info) return { language: "" };

  const langRangePath = info.match(/^([A-Za-z0-9_+-]+):(\d+):(\d+):(.+)$/);
  if (langRangePath && looksLikeFilePath(langRangePath[4] ?? "")) {
    return {
      language: langRangePath[1] ?? "",
      path: (langRangePath[4] ?? "").trim(),
      startLine: Number(langRangePath[2]),
      endLine: Number(langRangePath[3]),
    };
  }

  const rangePath = info.match(/^(\d+):(\d+):(.+)$/);
  if (rangePath && looksLikeFilePath(rangePath[3] ?? "")) {
    const path = (rangePath[3] ?? "").trim();
    return {
      language: languageFromPath(path),
      path,
      startLine: Number(rangePath[1]),
      endLine: Number(rangePath[2]),
    };
  }

  const linePath = info.match(/^(\d+):(.+)$/);
  if (linePath && looksLikeFilePath(linePath[2] ?? "")) {
    const path = (linePath[2] ?? "").trim();
    const startLine = Number(linePath[1]);
    return {
      language: languageFromPath(path),
      path,
      startLine,
      endLine: startLine,
    };
  }

  if (looksLikeFilePath(info)) {
    return { language: languageFromPath(info), path: info };
  }

  return { language: info };
}

export function splitWorkspacePaths(text: string): PathSegment[] {
  const segments: PathSegment[] = [];
  let last = 0;
  for (const match of text.matchAll(PATH_PATTERN)) {
    const index = match.index ?? 0;
    const previous = index > 0 ? text[index - 1] : "";
    if (previous && /[\w./:\\-]/.test(previous)) continue;
    if (index > last) segments.push({ type: "text", text: text.slice(last, index) });
    const startLine = match[2] ? Number(match[2]) : undefined;
    const endLine = match[3] ? Number(match[3]) : undefined;
    segments.push({
      type: "path",
      text: match[0],
      path: match[1],
      startLine,
      endLine,
    });
    last = index + match[0].length;
  }
  if (!segments.length) return [{ type: "text", text }];
  if (last < text.length) segments.push({ type: "text", text: text.slice(last) });
  return segments;
}
