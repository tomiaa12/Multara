import { t } from "../../i18n";

export type ComposerFileChipNode = {
  type: "chip";
  id: string;
  path: string;
  startLine: number;
  endLine: number;
  text: string;
  isDirectory?: boolean;
  source?: "terminal";
  /** 工作区内的相对路径要靠它拼成绝对路径；系统拖入的绝对路径可以不带。 */
  workspaceId?: string;
};

export type PromptWorkspace = {
  id: string;
  workPath?: string | null;
  sourcePath?: string | null;
};

export type ComposerEntityKind = "agent" | "squad" | "skill" | "mcp" | "task" | "requirement" | "session";

export type ComposerReferenceChipNode = {
  type: "chip";
  id: string;
  referenceId: string;
  referenceKind: ComposerEntityKind;
  label: string;
};

export type ComposerChipNode = ComposerFileChipNode | ComposerReferenceChipNode;

export type ComposerNode =
  | { type: "text"; value: string }
  | ComposerChipNode;

// 围栏长度可变（见 fencedBlock），收尾用反向引用匹配同长围栏；旧内容固定 ``` 同样能解析。
// 代码引用围栏可选：超大选区只发 path:range，不内联正文。
const CITATION_PATTERN =
  /\[代码引用\][ \t]+(.+):(\d+)(?:-(\d+))?(?:\n(`{3,})[^\n]*\n([\s\S]*?)\4(?!`))?/g;
const FOLDER_PATTERN =
  /\[文件夹\][ \t]+([^\n]+)(?:\n(`{3,})[^\n]*\n([\s\S]*?)\2(?!`))?/g;
const FILE_PATTERN =
  /\[文件\][ \t]+([^\n]+)(?:\n(`{3,})[^\n]*\n([\s\S]*?)\2(?!`))?/g;
const MIN_FENCE_LENGTH = 3;

/** 选区内联正文行数上限；超过则只发路径+行号（参考 VS Code 超预算丢弃选区正文）。 */
export const MAX_INLINE_SELECTION_LINES = 400;
/** 选区内联正文字节近似上限（按 UTF-16 length）。 */
export const MAX_INLINE_SELECTION_CHARS = 32_000;

export function isOversizedInlineSelection(
  text: string,
  startLine: number,
  endLine: number,
): boolean {
  const lineSpan = Math.max(0, endLine - startLine) + 1;
  const textLines = text.length ? text.split(/\r\n|\r|\n/).length : 0;
  const lines = Math.max(lineSpan, textLines);
  return lines > MAX_INLINE_SELECTION_LINES || text.length > MAX_INLINE_SELECTION_CHARS;
}

/** 围栏比内容里最长的连续反引号多一个，内容含 ``` 时不会提前截断。 */
export function fencedBlock(text: string) {
  const longest = (text.match(/`+/g) ?? []).reduce((max, run) => Math.max(max, run.length), 0);
  const fence = "`".repeat(Math.max(MIN_FENCE_LENGTH, longest + 1));
  return `${fence}\n${text}\n${fence}`;
}
const REFERENCE_PATTERN =
  /\[实体引用\][ \t]+(agent|squad|skill|mcp|task|requirement|session)[ \t]+([^\s]+)[ \t]+([^\n]+)/g;
const ASSIGNMENT_PATTERN =
  /\[指派要求\][^\n]*\n(?:-[^\n]*\n?)+/g;

export function isReferenceChip(node: ComposerNode): node is ComposerReferenceChipNode {
  return node.type === "chip" && "referenceKind" in node;
}

/**
 * 发出的消息 @ 了会话目标以外的智能体/小队时，把这些 @ 带到下一条输入框里：
 * 追问默认还发给刚才点名的对象，用户删掉 @ 才回到会话目标。
 * `sessionTarget` 形如 `agent:<id>` / `squad:<id>`。
 */
export function carriedMentions(nodes: ComposerNode[], sessionTarget: string): ComposerNode[] {
  const seen = new Set<string>();
  const chips = nodes.filter((node): node is ComposerReferenceChipNode => {
    if (!isReferenceChip(node) || (node.referenceKind !== "agent" && node.referenceKind !== "squad")) return false;
    const key = `${node.referenceKind}:${node.referenceId}`;
    if (key === sessionTarget || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  return chips.length ? [...chips, { type: "text", value: " " }] : [];
}

export function isFileChip(node: ComposerNode): node is ComposerFileChipNode {
  return node.type === "chip" && "path" in node;
}

export function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

export function lineRange(item: { startLine: number; endLine: number }) {
  return item.startLine === item.endLine
    ? `(${item.startLine})`
    : `(${item.startLine}-${item.endLine})`;
}

export function isWholeFileChip(item: { startLine: number; endLine: number; isDirectory?: boolean }) {
  return !item.isDirectory && item.startLine < 1;
}

const CHIP_MARK = "\u2060";

const REFERENCE_PREFIX: Record<ComposerEntityKind, string> = {
  agent: "@",
  squad: "@",
  skill: "/",
  mcp: "",
  task: "#",
  requirement: "",
  session: "",
};

export function chipLabel(item: ComposerChipNode) {
  if (isReferenceChip(item)) {
    if (item.referenceKind === "requirement") return t("chat.chip.requirement", { name: item.label });
    if (item.referenceKind === "session") return t("chat.chip.session", { name: item.label });
    const prefix = REFERENCE_PREFIX[item.referenceKind];
    return `${prefix}${item.label}`;
  }
  if (item.source === "terminal") return t("chat.chip.terminal", { range: lineRange(item) });
  if (item.isDirectory || isWholeFileChip(item)) return fileName(item.path) || item.path;
  return `${fileName(item.path)} ${lineRange(item)}`;
}

function markedChipLabel(item: ComposerChipNode) {
  return `${CHIP_MARK}${chipLabel(item)}${CHIP_MARK}`;
}

function cleanVisibleText(value: string) {
  return value.replaceAll(CHIP_MARK, "");
}

function decodeReferencePart(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function isAbsolutePath(path: string) {
  return path.startsWith("/") || /^[A-Za-z]:\//.test(path);
}

function workspaceRoot(workspaceId: string | undefined, workspaces: PromptWorkspace[]) {
  if (!workspaceId) return "";
  const workspace = workspaces.find((item) => item.id === workspaceId);
  return (workspace?.workPath || workspace?.sourcePath || "").replaceAll("\\", "/").replace(/\/+$/, "");
}

/**
 * 发给智能体的路径。界面只显示短名字，这里必须是磁盘上的位置。
 * 已是绝对路径就原样用；工作区内的相对路径拼上该工作区根目录。
 * 传了工作区列表、却只剩一个对不上的名字（没有斜杠）时返回空，避免把 `work` 当成目录。
 */
export function locatedChipPath(node: { path: string; workspaceId?: string }, workspaces: PromptWorkspace[] = []) {
  const path = node.path.replaceAll("\\", "/").replace(/\/+$/, "");
  if (!path) return "";
  if (isAbsolutePath(path)) return path;
  const root = workspaceRoot(node.workspaceId, workspaces);
  if (root) return `${root}/${path}`;
  if (workspaces.length && !path.includes("/")) return "";
  return path;
}

export function formatAgentPrompt(
  nodes: ComposerNode[],
  options: { directMentions?: boolean; workspaces?: PromptWorkspace[] } = {},
) {
  const assignees = nodes.flatMap((node) =>
    isReferenceChip(node) && (node.referenceKind === "agent" || node.referenceKind === "squad")
      ? [`${node.referenceKind}:${node.referenceId}:${node.label}`]
      : [],
  );
  const citations = nodes.flatMap((node) => {
    if (isReferenceChip(node)) {
      return [`[实体引用] ${node.referenceKind} ${encodeURIComponent(node.referenceId)} ${encodeURIComponent(node.label)}`];
    }
    if (!isFileChip(node) || !node.path) return [];
    const located = node.source === "terminal" ? "" : locatedChipPath(node, options.workspaces ?? []);
    if (node.source !== "terminal" && !located) return [];
    if (node.source === "terminal") {
      if (!node.text.trim() || isOversizedInlineSelection(node.text, node.startLine, node.endLine)) {
        return [];
      }
      const location = node.startLine === node.endLine ? `${node.startLine}` : `${node.startLine}-${node.endLine}`;
      return [`[终端] ${location}\n${fencedBlock(node.text)}`];
    }
    // 整文件 / 文件夹只发路径引用（对齐 VS Code Agent Host），正文由运行时按需读取。
    // 小选区仍内联；超大选区只发 path:range。历史全文围栏仍可由 parseUserPromptNodes 回放。
    if (node.isDirectory) {
      return [`[文件夹] ${located}`];
    }
    if (isWholeFileChip(node)) {
      return [`[文件] ${located}`];
    }
    const location = node.startLine === node.endLine
      ? `${located}:${node.startLine}`
      : `${located}:${node.startLine}-${node.endLine}`;
    const inline = node.text.trim()
      && !isOversizedInlineSelection(node.text, node.startLine, node.endLine);
    if (!inline) return [`[代码引用] ${location}`];
    return [`[代码引用] ${location}\n${fencedBlock(node.text)}`];
  });
  const visible = nodes
    .map((node) => (node.type === "chip" ? markedChipLabel(node) : node.value))
    .join("")
    .replace(/\u00a0/g, " ")
    .trim();
  const assignment = !options.directMentions && assignees.length
    ? `[指派要求] 以下是用户明确选择的指派对象，请由当前 Miku 或小队队长按现有任务流程创建并指派可追踪的子任务，不要只在正文中口头声称已分配：\n${assignees.map((item) => `- ${item}`).join("\n")}`
    : "";
  return [...citations, assignment, visible].filter((part) => part.trim()).join("\n\n");
}

export function visiblePromptText(content: string) {
  return parseUserPromptNodes(content)
    .map((node) => (node.type === "chip" ? chipLabel(node) : node.value))
    .join("")
    .replace(/\s+/g, " ")
    .trim();
}

export function parseUserPromptNodes(content: string): ComposerNode[] {
  const citations: ComposerChipNode[] = [];
  const visible = content
    .replace(ASSIGNMENT_PATTERN, "")
    .replace(REFERENCE_PATTERN, (_raw, referenceKind: ComposerEntityKind, referenceId: string, label: string) => {
      const decodedId = decodeReferencePart(referenceId);
      citations.push({
        type: "chip",
        id: `${referenceKind}:${decodedId}`,
        referenceId: decodedId,
        referenceKind,
        label: decodeReferencePart(label),
      });
      return "";
    })
    .replace(CITATION_PATTERN, (_raw, path: string, start: string, end: string | undefined, _fence: string | undefined, code: string | undefined) => {
      citations.push({
        type: "chip",
        id: `${path}:${start}-${end ?? start}`,
        path,
        startLine: Number(start),
        endLine: Number(end ?? start),
        text: code?.replace(/\n$/, "") ?? "",
      });
      return "";
    })
    .replace(FOLDER_PATTERN, (_raw, path: string, _fence: string | undefined, listing: string | undefined) => {
      citations.push({
        type: "chip",
        id: `${path}:folder`,
        path,
        startLine: 0,
        endLine: 0,
        text: listing?.replace(/\n$/, "") ?? "",
        isDirectory: true,
      });
      return "";
    })
    .replace(FILE_PATTERN, (_raw, path: string, _fence: string | undefined, content: string | undefined) => {
      citations.push({
        type: "chip",
        id: `${path}:file`,
        path,
        startLine: 0,
        endLine: 0,
        text: content?.replace(/\n$/, "") ?? "",
      });
      return "";
    })
    .replace(/^\n+/, "")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/\u00a0/g, " ");

  if (!citations.length) {
    const text = visible.trim();
    return text ? [{ type: "text", value: visible.replace(/^\n+/, "").replace(/\n+$/, "") || text }] : [];
  }

  const nodes: ComposerNode[] = [];
  let rest = visible.trim();
  for (const citation of citations) {
    const label = chipLabel(citation);
    const marked = markedChipLabel(citation);
    const markedIndex = rest.indexOf(marked);
    const index = markedIndex >= 0 ? markedIndex : rest.indexOf(label);
    const consumed = markedIndex >= 0 ? marked.length : label.length;
    if (index >= 0) {
      const before = cleanVisibleText(rest.slice(0, index));
      if (before) nodes.push({ type: "text", value: before });
      nodes.push(citation);
      rest = rest.slice(index + consumed);
    } else {
      nodes.push(citation);
    }
  }
  const tail = cleanVisibleText(rest);
  if (tail) nodes.push({ type: "text", value: tail });
  return nodes;
}

export function isPastedImage(attachment: { name: string; mimeType: string; path?: string }) {
  if (!attachment.mimeType.startsWith("image/")) return false;
  if (/^paste\./i.test(attachment.name)) return true;
  return Boolean(attachment.path?.replaceAll("\\", "/").includes("multara-paste"));
}
