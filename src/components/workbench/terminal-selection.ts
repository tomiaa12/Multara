/** 终端快照最多保留的行数，和行号偏移一起交给智能体。 */
export const TERMINAL_SNAPSHOT_LINE_CAP = 4000;

const ADD_BUTTON_WIDTH = 96;
const ADD_BUTTON_HEIGHT = 26;
const ADD_BUTTON_GAP = 6;

const PROMPT_LINE = /(?:^|\s)[%$#❯➜]\s*\S*$/;
/** PowerShell `PS C:\dir>` 与 cmd `C:\dir>`。 */
const WINDOWS_PROMPT_LINE = /^(?:PS )?[A-Za-z]:\\[^>]*>/;

export function isTerminalPrompt(line: string) {
  const text = line.trimEnd();
  return PROMPT_LINE.test(text) || WINDOWS_PROMPT_LINE.test(text);
}

/** 从被点中的行找到它所在的命令区块。最后一条还在输入的提示符不算。 */
export function terminalBlock(lines: string[], row: number) {
  if (row < 0 || row >= lines.length || !lines.length) return null;
  let start = row;
  while (start > 0 && !isTerminalPrompt(lines[start])) start -= 1;
  if (!isTerminalPrompt(lines[start])) return null;
  let end = start;
  while (end + 1 < lines.length && !isTerminalPrompt(lines[end + 1])) end += 1;
  let last = lines.length - 1;
  while (last > 0 && !lines[last]?.trim()) last -= 1;
  if (isTerminalPrompt(lines[last] ?? "") && start === last) return null;
  return { start, end };
}

export function selectionLines(startY: number, endY: number) {
  const startLine = Math.min(startY, endY) + 1;
  const endLine = Math.max(startY, endY) + 1;
  return { startLine, endLine };
}

export function snapshotLines(lines: string[]) {
  const start = Math.max(0, lines.length - TERMINAL_SNAPSHOT_LINE_CAP);
  const kept = lines.slice(start);
  while (kept.length > 0 && !kept[kept.length - 1]) kept.pop();
  return { lineOffset: start + 1, text: kept.join("\n") };
}

/** 按钮悬停在区块右上角外侧，不占终端行内的位置。区块顶超出可视区时贴在可视区右上角。 */
export function placeTerminalAddButton(
  block: { top: number },
  bounds: { width: number; height: number },
) {
  const x = Math.max(ADD_BUTTON_GAP, bounds.width - ADD_BUTTON_WIDTH - ADD_BUTTON_GAP);
  const hovered = block.top - ADD_BUTTON_HEIGHT - ADD_BUTTON_GAP;
  const y = hovered >= ADD_BUTTON_GAP
    ? hovered
    : Math.min(Math.max(block.top, ADD_BUTTON_GAP), Math.max(ADD_BUTTON_GAP, bounds.height - ADD_BUTTON_HEIGHT - ADD_BUTTON_GAP));
  return { x, y };
}
