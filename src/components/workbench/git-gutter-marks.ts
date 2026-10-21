import { computeLineDiff } from "../../utils/diff-engine";

export interface GitGutterMark {
  kind: "added" | "modified" | "deleted";
  line: number;
  atEnd: boolean;
}

export interface GitChange {
  kind: "added" | "modified" | "deleted";
  startLine: number;
  endLine: number;
  oldStart: number;
  oldLines: string[];
  newLines: string[];
}

export interface GitBaselineFile {
  inRepo: boolean;
  tracked: boolean;
  binary: boolean;
  content: string;
}

/** HEAD 里没有的文件（未跟踪）不画装饰，和 VS Code 的 quick diff 一样。 */
export function gitBaselineText(file: GitBaselineFile | null): string | null {
  if (!file || !file.inRepo || !file.tracked || file.binary) return null;
  return file.content;
}

export function gitGutterModel(
  baseline: string,
  current: string,
  lineCount: number,
): { marks: GitGutterMark[]; changes: GitChange[] } {
  if (lineCount <= 0 || baseline === current) return { marks: [], changes: [] };
  const marks: GitGutterMark[] = [];
  const changes: GitChange[] = [];
  for (const hunk of computeLineDiff(baseline, current)) {
    const oldCount = hunk.oldLines.length;
    const newCount = hunk.newLines.length;
    const hunkMarks: GitGutterMark[] = [];
    if (oldCount === 0) {
      for (let index = 0; index < newCount; index += 1) {
        hunkMarks.push({ kind: "added", line: hunk.newStart + index, atEnd: false });
      }
    } else if (newCount === 0) {
      hunkMarks.push(deletedMark(hunk.newStart, lineCount));
    } else {
      const paired = Math.min(oldCount, newCount);
      for (let index = 0; index < paired; index += 1) {
        hunkMarks.push({ kind: "modified", line: hunk.newStart + index, atEnd: false });
      }
      for (let index = paired; index < newCount; index += 1) {
        hunkMarks.push({ kind: "added", line: hunk.newStart + index, atEnd: false });
      }
      if (oldCount > newCount) hunkMarks.push(deletedMark(hunk.newStart + newCount, lineCount));
    }
    const visible = hunkMarks.filter((mark) => mark.line >= 1 && mark.line <= lineCount);
    if (visible.length === 0) continue;
    marks.push(...visible);
    changes.push({
      kind: oldCount === 0 ? "added" : newCount === 0 ? "deleted" : "modified",
      startLine: visible[0].line,
      endLine: visible[visible.length - 1].line,
      oldStart: hunk.oldStart,
      oldLines: hunk.oldLines,
      newLines: hunk.newLines,
    });
  }
  return { marks, changes };
}

export function gitGutterMarks(baseline: string, current: string, lineCount: number): GitGutterMark[] {
  return gitGutterModel(baseline, current, lineCount).marks;
}

export function gitChangeAtLine(changes: readonly GitChange[], line: number): number {
  return changes.findIndex((change) => line >= change.startLine && line <= change.endLine);
}

/** 对比区行数等于这块删掉和改后的行，最多 10 行。 */
export function gitPeekBodyLines(change: Pick<GitChange, "oldLines" | "newLines">): number {
  const lines = change.oldLines.length + change.newLines.length;
  return Math.min(10, Math.max(1, lines));
}

function deletedMark(line: number, lineCount: number): GitGutterMark {
  if (line > lineCount) return { kind: "deleted", line: lineCount, atEnd: true };
  if (line < 1) return { kind: "deleted", line: 1, atEnd: false };
  return { kind: "deleted", line, atEnd: false };
}

export function gitGutterClass(mark: GitGutterMark): string {
  const pattern = mark.kind === "modified" ? " pattern" : "";
  return `dirty-diff-glyph dirty-diff-${mark.kind} primary${pattern}`;
}

export function gitGutterHover(kind: GitGutterMark["kind"]): string {
  if (kind === "added") return "已添加的行";
  if (kind === "modified") return "已更改的行";
  return "已删除的行";
}
