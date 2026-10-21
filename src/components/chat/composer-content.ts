export type ComposerChipNode = {
  type: "chip";
  id: string;
  path: string;
  startLine: number;
  endLine: number;
  text: string;
  isDirectory?: boolean;
  source?: "terminal";
};

export type ComposerNode =
  | { type: "text"; value: string }
  | ComposerChipNode;

const CITATION_PATTERN =
  /\[代码引用\][ \t]+(.+):(\d+)(?:-(\d+))?\n```[^\n]*\n([\s\S]*?)```/g;
const FOLDER_PATTERN =
  /\[文件夹\][ \t]+([^\n]+)(?:\n```[^\n]*\n([\s\S]*?)```)?/g;
const FILE_PATTERN =
  /\[文件\][ \t]+([^\n]+)(?:\n```[^\n]*\n([\s\S]*?)```)?/g;

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

export function chipLabel(item: {
  path: string;
  startLine: number;
  endLine: number;
  isDirectory?: boolean;
  source?: "terminal";
}) {
  if (item.source === "terminal") return `终端 ${lineRange(item)}`;
  if (item.isDirectory || isWholeFileChip(item)) return fileName(item.path) || item.path;
  return `${fileName(item.path)} ${lineRange(item)}`;
}

function markedChipLabel(item: {
  path: string;
  startLine: number;
  endLine: number;
  isDirectory?: boolean;
}) {
  return `${CHIP_MARK}${chipLabel(item)}${CHIP_MARK}`;
}

function cleanVisibleText(value: string) {
  return value.replaceAll(CHIP_MARK, "");
}

export function formatAgentPrompt(nodes: ComposerNode[]) {
  const citations = nodes.flatMap((node) => {
    if (node.type !== "chip" || !node.path) return [];
    if (node.source === "terminal") {
      const location = node.startLine === node.endLine ? `${node.startLine}` : `${node.startLine}-${node.endLine}`;
      return [`[终端] ${location}\n\`\`\`\n${node.text}\n\`\`\``];
    }
    if (node.isDirectory) {
      return node.text.trim()
        ? [`[文件夹] ${node.path}\n\`\`\`\n${node.text}\n\`\`\``]
        : [`[文件夹] ${node.path}`];
    }
    if (isWholeFileChip(node)) {
      return node.text.trim()
        ? [`[文件] ${node.path}\n\`\`\`\n${node.text}\n\`\`\``]
        : [`[文件] ${node.path}`];
    }
    if (!node.text.trim()) return [];
    const location = node.startLine === node.endLine
      ? `${node.path}:${node.startLine}`
      : `${node.path}:${node.startLine}-${node.endLine}`;
    return [`[代码引用] ${location}\n\`\`\`\n${node.text}\n\`\`\``];
  });
  const visible = nodes
    .map((node) => (node.type === "chip" ? markedChipLabel(node) : node.value))
    .join("")
    .replace(/\u00a0/g, " ")
    .trim();
  return [...citations, visible].filter((part) => part.trim()).join("\n\n");
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
    .replace(CITATION_PATTERN, (_raw, path: string, start: string, end: string | undefined, code: string) => {
      citations.push({
        type: "chip",
        id: `${path}:${start}-${end ?? start}`,
        path,
        startLine: Number(start),
        endLine: Number(end ?? start),
        text: code.replace(/\n$/, ""),
      });
      return "";
    })
    .replace(FOLDER_PATTERN, (_raw, path: string, listing: string | undefined) => {
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
    .replace(FILE_PATTERN, (_raw, path: string, content: string | undefined) => {
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
