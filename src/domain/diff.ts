/** 一个连续变更区域（= git diff 的一个 hunk） */
export interface DiffHunk {
  /** 在基线中的起始行号（1-based） */
  oldStart: number;
  /** 基线中被删除/替换的原始行 */
  oldLines: string[];
  /** 在当前文件中的起始行号（1-based） */
  newStart: number;
  /** 当前文件中新增/替换的行 */
  newLines: string[];
}

export type DiffChunkStatus = "pending" | "accepted" | "undone";

/** 一个可独立操作的 Chunk */
export interface DiffChunk {
  chunkId: string;
  messageId: string;
  workspaceId: string;
  filePath: string;
  chunkKey: string;
  status: DiffChunkStatus;
  hunk: DiffHunk;
}

export interface TurnBaseline {
  messageId: string;
  workspaceId: string;
  baselineContent: string;
}

export interface SessionSnapshotEntry {
  messageId: string;
  workspaceId: string;
  filePath: string;
  content: string;
}

export interface SessionChunkState {
  chunkId: string;
  status: DiffChunkStatus;
}

export interface SessionDiffData {
  snapshots: SessionSnapshotEntry[];
  chunkStates: SessionChunkState[];
  messageIds: string[];
}

export interface ActiveSnapshotEntry extends SessionSnapshotEntry {
  sessionId: string;
}

export interface FileBaselineEntry {
  workspaceId: string;
  filePath: string;
  content: string;
}

export interface ActiveDiffData {
  snapshots: ActiveSnapshotEntry[];
  chunkStates: SessionChunkState[];
  baselines?: FileBaselineEntry[];
}

export interface SnapshotEntry {
  filePath: string;
  content: string;
}

/** FNV-1a 64-bit，取前 12 位 hex 作为 hunk 内容指纹 */
export function chunkKeyFor(hunk: Pick<DiffHunk, "oldLines" | "newLines">): string {
  const payload = `${hunk.oldLines.join("\n")}\0${hunk.newLines.join("\n")}`;
  return fnv1a64Hex(payload).slice(0, 12);
}

export function parseChunkId(chunkId: string): {
  messageId: string;
  workspaceId: string;
  filePath: string;
  chunkKey: string;
} | null {
  const parts = chunkId.split(":");
  if (parts.length < 4) return null;
  const chunkKey = parts[parts.length - 1];
  if (!chunkKey) return null;
  return {
    messageId: parts[0],
    workspaceId: parts[1],
    filePath: parts.slice(2, -1).join(":"),
    chunkKey,
  };
}

export function makeChunkId(
  messageId: string,
  workspaceId: string,
  filePath: string,
  chunkKey: string,
): string {
  return `${messageId}:${workspaceId}:${filePath}:${chunkKey}`;
}

function fnv1a64Hex(input: string): string {
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  for (let index = 0; index < input.length; index += 1) {
    hash ^= BigInt(input.charCodeAt(index));
    hash = (hash * prime) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, "0");
}
