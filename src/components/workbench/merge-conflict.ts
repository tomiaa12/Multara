export type MergeConflictChoice = "current" | "incoming" | "both";

export type MergeConflict = {
  headerLine: number;
  splitterLine: number;
  footerLine: number;
  currentName: string;
  incomingName: string;
};

const HEADER = "<<<<<<<";
const ANCESTOR = "|||||||";
const SPLITTER = "=======";
const FOOTER = ">>>>>>>";

function linesOf(text: string) {
  return text.split(/\r?\n/);
}

/** 扫描 <<<<<<< / ======= / >>>>>>>。不完整或嵌套的标记到此为止，和 VS Code 一样。 */
export function findMergeConflicts(text: string): MergeConflict[] {
  if (!text.includes(HEADER)) return [];
  const lines = linesOf(text);
  const conflicts: MergeConflict[] = [];
  let header = 0;
  let splitter = 0;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index] ?? "";
    const number = index + 1;
    if (line.startsWith(HEADER)) {
      if (header) break;
      header = number;
      splitter = 0;
      continue;
    }
    if (!header) continue;
    if (!splitter && line === SPLITTER) {
      splitter = number;
      continue;
    }
    if (!line.startsWith(FOOTER)) continue;
    if (splitter) {
      conflicts.push({
        headerLine: header,
        splitterLine: splitter,
        footerLine: number,
        currentName: (lines[header - 1] ?? "").slice(HEADER.length).trim(),
        incomingName: line.slice(FOOTER.length).trim(),
      });
    }
    header = 0;
    splitter = 0;
  }
  return conflicts;
}

function chosenBody(choice: MergeConflictChoice, current: string[], incoming: string[]) {
  if (choice === "current") return current;
  if (choice === "incoming") return incoming;
  return [...current, ...incoming];
}

function ancestorLine(lines: string[], conflict: MergeConflict) {
  for (let line = conflict.headerLine + 1; line < conflict.splitterLine; line += 1) {
    if ((lines[line - 1] ?? "").startsWith(ANCESTOR)) return line;
  }
  return conflict.splitterLine;
}

/** 用当前、传入或两边的正文替换整段冲突标记。 */
export function resolveMergeConflict(text: string, conflict: MergeConflict, choice: MergeConflictChoice) {
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const lines = linesOf(text);
  const header = lines[conflict.headerLine - 1] ?? "";
  const footer = lines[conflict.footerLine - 1] ?? "";
  if (!header.startsWith(HEADER) || !footer.startsWith(FOOTER)) return text;
  const ancestor = ancestorLine(lines, conflict);
  const current = lines.slice(conflict.headerLine, ancestor - 1);
  const incoming = lines.slice(conflict.splitterLine, conflict.footerLine - 1);
  const body = chosenBody(choice, current, incoming);
  return [
    ...lines.slice(0, conflict.headerLine - 1),
    ...body,
    ...lines.slice(conflict.footerLine),
  ].join(eol);
}
