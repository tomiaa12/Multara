export interface FileLinkCandidate {
  /** 在行文本里的起止下标（end 不含）。 */
  start: number;
  end: number;
  path: string;
  line?: number;
  column?: number;
}

export interface UrlLinkCandidate {
  start: number;
  end: number;
  url: string;
}

const URL = /https?:\/\/[^\s<>"'`]+/gi;

const PATH_CHAR = "[^\\s\"'`<>|*?()\\[\\]{},;:]";
const PATH_BODY = `(?:[A-Za-z]:[\\\\/]|\\.{1,2}[\\\\/]|~[\\\\/]|[\\\\/])?(?:${PATH_CHAR}+[\\\\/])*${PATH_CHAR}+`;
/** `:12`、`:12:3`、`(12)`、`(12,3)`、`", line 12"`。 */
const SUFFIX = "(?::(\\d+)(?::(\\d+))?|\\((\\d+)(?:,\\s*(\\d+))?\\)|\",\\s*line\\s+(\\d+))?";
const LINK = new RegExp(`(${PATH_BODY})${SUFFIX}`, "g");

const EXTENSION = /\.[A-Za-z][\w-]{0,9}$/;
const MAX_LINKS_PER_LINE = 20;

function looksLikePath(path: string) {
  if (path.length < 2) return false;
  if (/^\d+(\.\d+)*$/.test(path)) return false;
  const hasSeparator = /[\\/]/.test(path);
  if (!hasSeparator) return EXTENSION.test(path);
  // 单独的 / 或 // 之类没有意义
  return /[\w\u4e00-\u9fff]/.test(path);
}

/** 找出一行里可能是文件路径的片段；是否存在由调用方再到磁盘上确认。 */
export function detectFileLinks(text: string): FileLinkCandidate[] {
  const result: FileLinkCandidate[] = [];
  LINK.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = LINK.exec(text)) && result.length < MAX_LINKS_PER_LINE) {
    if (match[0] === "") {
      LINK.lastIndex += 1;
      continue;
    }
    const raw = match[1]!;
    const start = match.index;
    // 处在 URL 里的片段交给网址链接处理
    const tokenStart = Math.max(text.lastIndexOf(" ", start - 1), text.lastIndexOf("\t", start - 1)) + 1;
    if (/[a-z][\w+.-]*:\/\//i.test(text.slice(tokenStart, start + raw.length + 3))) continue;
    const path = raw.replace(/[.]+$/, "");
    if (!looksLikePath(path)) continue;
    const line = Number(match[2] ?? match[4] ?? match[6]) || undefined;
    const column = Number(match[3] ?? match[5]) || undefined;
    const trimmed = raw.length - path.length;
    const end = trimmed > 0 && !line ? start + path.length : start + match[0].length;
    result.push({ start, end, path, line, column });
  }
  return result;
}

/** 找出一行里的网址，去掉句末标点。 */
export function detectUrlLinks(text: string): UrlLinkCandidate[] {
  const result: UrlLinkCandidate[] = [];
  URL.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = URL.exec(text)) && result.length < MAX_LINKS_PER_LINE) {
    let url = match[0];
    let end = match.index + url.length;
    while (url && /[),.;:!?]$/.test(url)) {
      url = url.slice(0, -1);
      end -= 1;
    }
    if (url.length < 10) continue;
    result.push({ start: match.index, end, url });
  }
  return result;
}
