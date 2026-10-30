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

/** 文件列表变化时算一次的小写路径与文件名，按键时不再对每个文件重复 replaceAll / toLowerCase / split。 */
export interface QuickOpenCandidate {
  file: QuickOpenFile;
  hay: string;
  name: string;
}

const pathCollator = new Intl.Collator();

function candidateOf(file: QuickOpenFile): QuickOpenCandidate {
  const hay = file.path.replaceAll("\\", "/").toLowerCase();
  return { file, hay, name: hay.slice(hay.lastIndexOf("/") + 1) };
}

export function prepareQuickOpenFiles(files: QuickOpenFile[]): QuickOpenCandidate[] {
  return files.map(candidateOf);
}

function queryTokens(filter: string) {
  const query = filter.trim().toLowerCase();
  return query ? query.split(/\s+/) : [];
}

function scoreTokens(hay: string, name: string, tokens: string[]): number | null {
  let score = 0;
  for (const token of tokens) {
    const part = scoreToken(hay, name, token);
    if (part === null) return null;
    score += part;
  }
  return score;
}

export function scoreQuickOpenPath(path: string, filter: string): number | null {
  const { hay, name } = candidateOf({ workspaceId: "", path, label: "", detail: "" });
  return scoreTokens(hay, name, queryTokens(filter));
}

export function rankPreparedQuickOpenFiles(candidates: QuickOpenCandidate[], filter: string): QuickOpenFile[] {
  const tokens = queryTokens(filter);
  const ranked: Array<{ file: QuickOpenFile; score: number }> = [];
  for (const candidate of candidates) {
    const score = scoreTokens(candidate.hay, candidate.name, tokens);
    if (score !== null) ranked.push({ file: candidate.file, score });
  }
  ranked.sort((a, b) => b.score - a.score || pathCollator.compare(a.file.path, b.file.path));
  return ranked.slice(0, MAX_RESULTS).map((item) => item.file);
}

export function rankQuickOpenFiles(files: QuickOpenFile[], filter: string): QuickOpenFile[] {
  return rankPreparedQuickOpenFiles(prepareQuickOpenFiles(files), filter);
}
