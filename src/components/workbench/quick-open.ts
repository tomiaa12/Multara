export interface QuickOpenQuery {
  filter: string;
  startLine?: number;
  endLine?: number;
}

export interface QuickOpenFile {
  workspaceId: string;
  path: string;
  label: string;
  detail: string;
}

/** 与 VS Code Quick Open 相同：路径末尾的 `:行`、`:行:列`、`:行-行` 不参与文件名匹配。 */
const LINE_COLON_PATTERN = /\s?[#:(](?:line )?(\d+)(?:[#:,](\d*))?(?:-(\d+)(?:[#:,](\d*))?)?\)?:?\s*$/;

const MAX_RESULTS = 100;

export function parseQuickOpenQuery(raw: string): QuickOpenQuery {
  const match = LINE_COLON_PATTERN.exec(raw);
  if (!match || match.index === undefined || match.index <= 0) return { filter: raw.trim() };
  const startLine = Number(match[1]);
  if (!Number.isFinite(startLine) || startLine <= 0) return { filter: raw.trim() };
  const endLine = match[3] ? Number(match[3]) : startLine;
  return {
    filter: raw.slice(0, match.index).trim(),
    startLine,
    endLine: Number.isFinite(endLine) && endLine > 0 ? endLine : startLine,
  };
}

function scoreToken(path: string, name: string, token: string): number | null {
  const needle = token.replace(/\/+$/, "");
  if (!needle) return 0;
  const inName = name.indexOf(needle);
  if (inName >= 0) return 120 - Math.min(inName, 80);
  const inPath = path.indexOf(needle);
  if (inPath < 0) return null;
  const directory = token.endsWith("/") || path.slice(inPath + needle.length).startsWith("/");
  return directory ? 80 - Math.min(inPath, 40) : 40 - Math.min(inPath, 30);
}

export function scoreQuickOpenPath(path: string, filter: string): number | null {
  const query = filter.trim().toLowerCase();
  if (!query) return 0;
  const hay = path.replaceAll("\\", "/").toLowerCase();
  const name = hay.split("/").pop() ?? hay;
  let score = 0;
  for (const token of query.split(/\s+/)) {
    const part = scoreToken(hay, name, token);
    if (part === null) return null;
    score += part;
  }
  return score;
}

export function rankQuickOpenFiles(files: QuickOpenFile[], filter: string): QuickOpenFile[] {
  const ranked = files
    .map((file) => ({ file, score: scoreQuickOpenPath(file.path, filter) }))
    .filter((item): item is { file: QuickOpenFile; score: number } => item.score !== null)
    .sort((a, b) => b.score - a.score || a.file.path.localeCompare(b.file.path));
  return ranked.slice(0, MAX_RESULTS).map((item) => item.file);
}
