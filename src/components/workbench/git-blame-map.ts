import { computeLineDiff } from "../../utils/diff-engine";

/**
 * 把当前缓冲区行号映回 blame 缓存时的行号。
 * 纯新增行返回 null；未改 / 配对修改行返回基线行号。
 */
export function mapCurrentLineToBlameLine(
  base: string,
  current: string,
  currentLine: number,
): number | null {
  if (currentLine < 1) return null;
  if (base === current) return currentLine;

  const hunks = computeLineDiff(base, current);
  let baseCursor = 1;
  let curCursor = 1;

  for (const hunk of hunks) {
    while (curCursor < hunk.newStart) {
      if (curCursor === currentLine) return baseCursor;
      curCursor += 1;
      baseCursor += 1;
    }
    const paired = Math.min(hunk.oldLines.length, hunk.newLines.length);
    for (let index = 0; index < hunk.newLines.length; index += 1) {
      if (curCursor === currentLine) {
        return index < paired ? baseCursor + index : null;
      }
      curCursor += 1;
    }
    baseCursor += hunk.oldLines.length;
  }

  if (currentLine >= curCursor) return baseCursor + (currentLine - curCursor);
  return null;
}
