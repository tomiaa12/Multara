/** GitLens Search & Compare：搜索语法提示与查询解析。 */

export type SearchHelpItem = {
  id: string;
  /** 左侧标签，如「按提交信息搜索」 */
  labelKey: string;
  /** 右侧语法说明 */
  detailKey: string;
  /** 点击后预填到输入框 */
  insert: string;
  /** 分组分隔：该项前画分割线 */
  dividerBefore?: boolean;
};

export const SEARCH_HELP_ITEMS: SearchHelpItem[] = [
  {
    id: "message",
    labelKey: "scm.searchCompare.help.message",
    detailKey: "scm.searchCompare.help.messageDetail",
    insert: "message:",
  },
  {
    id: "excludeMessage",
    labelKey: "scm.searchCompare.help.excludeMessage",
    detailKey: "scm.searchCompare.help.excludeMessageDetail",
    insert: "-message:",
  },
  {
    id: "author",
    labelKey: "scm.searchCompare.help.author",
    detailKey: "scm.searchCompare.help.authorDetail",
    insert: "author:",
  },
  {
    id: "committer",
    labelKey: "scm.searchCompare.help.committer",
    detailKey: "scm.searchCompare.help.committerDetail",
    insert: "committer:",
  },
  {
    id: "sha",
    labelKey: "scm.searchCompare.help.sha",
    detailKey: "scm.searchCompare.help.shaDetail",
    insert: "commit:",
  },
  {
    id: "ref",
    labelKey: "scm.searchCompare.help.ref",
    detailKey: "scm.searchCompare.help.refDetail",
    insert: "ref:",
  },
  {
    id: "type",
    labelKey: "scm.searchCompare.help.type",
    detailKey: "scm.searchCompare.help.typeDetail",
    insert: "type:",
  },
  {
    id: "file",
    labelKey: "scm.searchCompare.help.file",
    detailKey: "scm.searchCompare.help.fileDetail",
    insert: "file:",
    dividerBefore: true,
  },
  {
    id: "change",
    labelKey: "scm.searchCompare.help.change",
    detailKey: "scm.searchCompare.help.changeDetail",
    insert: "change:",
  },
  {
    id: "after",
    labelKey: "scm.searchCompare.help.after",
    detailKey: "scm.searchCompare.help.afterDetail",
    insert: "after:",
    dividerBefore: true,
  },
  {
    id: "before",
    labelKey: "scm.searchCompare.help.before",
    detailKey: "scm.searchCompare.help.beforeDetail",
    insert: "before:",
  },
];

export type ParsedSearch = {
  raw: string;
  grep?: string;
  excludeGrep?: string;
  author?: string;
  committer?: string;
  sha?: string;
  ref?: string;
  file?: string;
  change?: string;
  free?: string;
};

const TOKEN_RE =
  /(-?message:|=:|author:|@:|committer:|commit:|#:|ref:|\^:|file:|\?:|change:|~:|after:|since:|before:|until:|type:|is:)("(?:\\.|[^"])*"|[^\s]*)/gi;

function stripQuotes(value: string) {
  const trimmed = value.trim();
  if (trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"')) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

/** 解析 GitLens 风格查询。 */
export function parseSearchQuery(input: string): ParsedSearch {
  const raw = input.trim();
  if (!raw) return { raw: "" };

  const out: ParsedSearch = { raw };
  const leftovers: string[] = [];
  let cursor = 0;
  TOKEN_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = TOKEN_RE.exec(raw))) {
    if (match.index > cursor) {
      leftovers.push(raw.slice(cursor, match.index));
    }
    cursor = match.index + match[0].length;
    const prefix = match[1].toLowerCase();
    const value = stripQuotes(match[2] ?? "");
    if (!value && prefix !== "type:" && prefix !== "is:") continue;
    if (prefix === "message:" || prefix === "=:") out.grep = value;
    else if (prefix === "-message:") out.excludeGrep = value;
    else if (prefix === "author:" || prefix === "@:") out.author = value;
    else if (prefix === "committer:") out.committer = value;
    else if (prefix === "commit:" || prefix === "#:") out.sha = value;
    else if (prefix === "ref:" || prefix === "^:") out.ref = value;
    else if (prefix === "file:" || prefix === "?:") out.file = value;
    else if (prefix === "change:" || prefix === "~:") out.change = value;
  }
  if (cursor < raw.length) leftovers.push(raw.slice(cursor));
  const free = leftovers.join(" ").replace(/\s+/g, " ").trim();
  if (free) out.free = free;
  return out;
}
