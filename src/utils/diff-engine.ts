import { structuredPatch } from "diff";
import type { DiffChunk, DiffChunkStatus, DiffHunk, TurnBaseline } from "../domain/diff";
import { chunkKeyFor, makeChunkId } from "../domain/diff";

export function computeLineDiff(baseline: string, current: string): DiffHunk[] {
  if (baseline === current) return [];

  const patch = structuredPatch("", "", baseline, current, "", "", {
    context: 0,
  });

  return patch.hunks.map((hunk) => {
    const oldLines = patchSignedLines(hunk.lines, "-");
    const newLines = patchSignedLines(hunk.lines, "+");

    return {
      oldStart: hunk.oldStart,
      oldLines,
      newStart: hunk.newStart,
      newLines,
    };
  });
}

function patchSignedLines(lines: string[], prefix: "+" | "-"): string[] {
  return lines.filter((line) => line.startsWith(prefix)).map((line) => line.slice(1));
}

/** Undo 用：把 current 中 hunk 对应的新增行替换回基线的原始行。 */
export function applyReversePatch(current: string, hunk: DiffHunk): string {
  const lines = current.split("\n");
  const start = Math.max(0, hunk.newStart - 1);
  const deleteCount = hunk.newLines.length;
  lines.splice(start, deleteCount, ...hunk.oldLines);
  return lines.join("\n");
}

/** Keep 用：把这块改动写进 baseline，相当于把 original 侧推到 modified。 */
export function applyKeepHunk(baseline: string, hunk: DiffHunk): string {
  const lines = baseline === "" ? [] : baseline.split("\n");
  const start = hunk.oldStart <= 0 ? 0 : hunk.oldStart - 1;
  lines.splice(start, hunk.oldLines.length, ...hunk.newLines);
  return lines.join("\n");
}

export function reviewChunkId(workspaceId: string, filePath: string, hunk: DiffHunk): string {
  return makeChunkId("file", workspaceId, filePath, chunkKeyFor(hunk));
}

export function hunkVisualKey(hunk: DiffHunk) {
  return `${hunk.newStart}\0${hunk.oldLines.join("\n")}\0${hunk.newLines.join("\n")}`;
}

export function listAllChunksForFile(
  workspaceId: string,
  filePath: string,
  currentContent: string,
  turns: TurnBaseline[],
  chunkStates: Map<string, DiffChunkStatus>,
): DiffChunk[] {
  const result: DiffChunk[] = [];
  for (const { messageId, baselineContent } of turns) {
    for (const hunk of computeLineDiff(baselineContent, currentContent)) {
      const chunkKey = chunkKeyFor(hunk);
      const chunkId = makeChunkId(messageId, workspaceId, filePath, chunkKey);
      result.push({
        chunkId,
        messageId,
        workspaceId,
        filePath,
        chunkKey,
        status: chunkStates.get(chunkId) ?? "pending",
        hunk,
      });
    }
  }
  return result;
}

/**
 * 展示用 pending chunks：重叠区域归最新 turn；视觉完全相同的 hunk 只留一份。
 */
export function getActiveChunksForFile(
  workspaceId: string,
  filePath: string,
  currentContent: string,
  turns: TurnBaseline[],
  chunkStates: Map<string, DiffChunkStatus>,
): DiffChunk[] {
  const pending = listAllChunksForFile(
    workspaceId,
    filePath,
    currentContent,
    turns,
    chunkStates,
  ).filter((chunk) => chunk.status === "pending");
  return dedupeVisualChunks(pending).sort((a, b) => a.hunk.newStart - b.hunk.newStart);
}

export function matchingVisualChunkIds(
  workspaceId: string,
  filePath: string,
  currentContent: string,
  turns: TurnBaseline[],
  chunkStates: Map<string, DiffChunkStatus>,
  visualKey: string,
): string[] {
  const ids: string[] = [];
  const seen = new Set<string>();
  for (const chunk of listAllChunksForFile(
    workspaceId,
    filePath,
    currentContent,
    turns,
    chunkStates,
  )) {
    if (chunk.status !== "pending") continue;
    if (hunkVisualKey(chunk.hunk) !== visualKey) continue;
    if (seen.has(chunk.chunkId)) continue;
    seen.add(chunk.chunkId);
    ids.push(chunk.chunkId);
  }
  return ids;
}

function dedupeVisualChunks(chunks: DiffChunk[]): DiffChunk[] {
  const seen = new Set<string>();
  const result: DiffChunk[] = [];
  for (const chunk of chunks) {
    const key = hunkVisualKey(chunk.hunk);
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(chunk);
  }
  return result;
}
