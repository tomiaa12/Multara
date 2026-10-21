import { defineStore } from "pinia";
import { ref } from "vue";
import * as backend from "../services/backend";
import { chunkKeyFor, parseChunkId } from "../domain/diff";
import {
  applyKeepHunk,
  applyReversePatch,
  computeLineDiff,
  reviewChunkId,
} from "../utils/diff-engine";
import type { DiffChunk, FileBaselineEntry } from "../domain/diff";

export type PendingDiffFile = { workspaceId: string; filePath: string };

function fileKey(workspaceId: string, filePath: string) {
  return `${workspaceId}\n${filePath}`;
}

function parseFileKey(key: string): PendingDiffFile | null {
  const sep = key.indexOf("\n");
  if (sep < 0) return null;
  const filePath = key.slice(sep + 1);
  if (!filePath) return null;
  return { workspaceId: key.slice(0, sep), filePath };
}

export const useDiffStore = defineStore("diff", () => {
  /**
   * sessionId → messageId → `${workspaceId}\\n${filePath}` → baselineContent
   */
  const snapshots = ref(new Map<string, Map<string, Map<string, string>>>());
  /** workspaceId + path → 待审阅原文。跟文件走，不跟当前对话走。 */
  const baselines = ref(new Map<string, string>());
  const chunkStates = ref(new Map<string, "pending" | "accepted" | "undone">());
  const reviewMemory = new Map<string, string>();
  const reviewQueue = new Map<string, Promise<unknown>>();
  const turnOrder = ref(new Map<string, string[]>());
  const activeSessionId = ref("");
  const diskEpoch = ref(0);
  const missingFiles = ref(new Set<string>());
  const dismissedMissing = ref(new Set<string>());

  function applySessionData(
    sessionId: string,
    data: Awaited<ReturnType<typeof backend.loadSessionDiffData>>,
    nextSnapshots: Map<string, Map<string, Map<string, string>>>,
    nextOrder: Map<string, string[]>,
    nextStates: Map<string, "pending" | "accepted" | "undone">,
  ) {
    const snapshotMap = new Map<string, Map<string, string>>();
    for (const { messageId, workspaceId, filePath, content } of data.snapshots) {
      if (!snapshotMap.has(messageId)) snapshotMap.set(messageId, new Map());
      snapshotMap.get(messageId)!.set(fileKey(workspaceId, filePath), content);
    }
    nextSnapshots.set(sessionId, snapshotMap);
    nextOrder.set(sessionId, data.messageIds);
    for (const { chunkId, status } of data.chunkStates) {
      nextStates.set(chunkId, status);
    }
  }

  async function loadSessions(sessionIds: string[]) {
    const nextSnapshots = new Map(snapshots.value);
    const nextOrder = new Map(turnOrder.value);
    const nextStates = new Map(chunkStates.value);
    for (const sessionId of sessionIds) {
      const data = await backend.loadSessionDiffData(sessionId);
      applySessionData(sessionId, data, nextSnapshots, nextOrder, nextStates);
    }
    snapshots.value = nextSnapshots;
    turnOrder.value = nextOrder;
    chunkStates.value = nextStates;
    for (const sessionId of sessionIds) adoptEarliestBaseline(sessionId);
  }

  function adoptEarliestBaseline(sessionId: string) {
    const sessionSnaps = snapshots.value.get(sessionId);
    if (!sessionSnaps) return;
    const next = new Map(baselines.value);
    let changed = false;
    for (const messageId of turnOrder.value.get(sessionId) ?? []) {
      const fileMap = sessionSnaps.get(messageId);
      if (!fileMap) continue;
      for (const [key, content] of fileMap) {
        if (next.has(key)) continue;
        next.set(key, content);
        changed = true;
      }
    }
    if (changed) baselines.value = next;
  }

  function replaceBaselines(entries: FileBaselineEntry[]) {
    const next = new Map<string, string>();
    for (const entry of entries) {
      if (!entry.filePath) continue;
      next.set(fileKey(entry.workspaceId, entry.filePath), entry.content);
    }
    baselines.value = next;
  }

  function enqueueReview<T>(workspaceId: string, filePath: string, job: () => Promise<T>): Promise<T> {
    const key = fileKey(workspaceId, filePath);
    const previous = reviewQueue.get(key) ?? Promise.resolve();
    const run = previous.then(job, job);
    reviewQueue.set(
      key,
      run.then(
        () => undefined,
        () => undefined,
      ),
    );
    return run;
  }

  async function loadSession(sessionId: string) {
    activeSessionId.value = sessionId;
    await loadSessions([sessionId]);
  }

  async function loadActive() {
    const data = await backend.loadActiveDiffData();
    const nextSnapshots = new Map<string, Map<string, Map<string, string>>>();
    const nextOrder = new Map<string, string[]>();
    const nextStates = new Map<string, "pending" | "accepted" | "undone">();
    for (const { sessionId, messageId, workspaceId, filePath, content } of data.snapshots) {
      if (!sessionId || !filePath) continue;
      if (!nextSnapshots.has(sessionId)) nextSnapshots.set(sessionId, new Map());
      const sessionSnaps = nextSnapshots.get(sessionId)!;
      if (!sessionSnaps.has(messageId)) sessionSnaps.set(messageId, new Map());
      sessionSnaps.get(messageId)!.set(fileKey(workspaceId, filePath), content);
      const order = nextOrder.get(sessionId) ?? [];
      if (!order.includes(messageId)) nextOrder.set(sessionId, [...order, messageId]);
    }
    for (const { chunkId, status } of data.chunkStates) {
      nextStates.set(chunkId, status);
    }
    snapshots.value = nextSnapshots;
    turnOrder.value = nextOrder;
    chunkStates.value = nextStates;
    if (Array.isArray(data.baselines)) {
      replaceBaselines(data.baselines);
    } else {
      for (const sessionId of nextOrder.keys()) adoptEarliestBaseline(sessionId);
    }
  }

  function baselineContent(workspaceId: string, filePath: string) {
    return baselines.value.get(fileKey(workspaceId, filePath));
  }

  function chunksForBaseline(
    workspaceId: string,
    filePath: string,
    currentContent: string,
  ): DiffChunk[] {
    const baseline = baselineContent(workspaceId, filePath);
    if (baseline === undefined || baseline === currentContent) return [];
    return computeLineDiff(baseline, currentContent).map((hunk) => ({
      chunkId: reviewChunkId(workspaceId, filePath, hunk),
      messageId: "file",
      workspaceId,
      filePath,
      chunkKey: chunkKeyFor(hunk),
      status: "pending" as const,
      hunk,
    }));
  }

  function getChunksForFile(
    workspaceId: string,
    filePath: string,
    currentContent: string,
    _sessionId?: string,
  ): DiffChunk[] {
    return chunksForBaseline(workspaceId, filePath, currentContent);
  }

  function listSnapshotFiles(
    workspaceIds: string[],
    _sessionId?: string,
  ): PendingDiffFile[] {
    const allowed = new Set(workspaceIds);
    const files: PendingDiffFile[] = [];
    for (const key of baselines.value.keys()) {
      const parsed = parseFileKey(key);
      if (!parsed || (allowed.size && !allowed.has(parsed.workspaceId))) continue;
      files.push(parsed);
    }
    return files.sort((left, right) =>
      left.filePath.localeCompare(right.filePath) || left.workspaceId.localeCompare(right.workspaceId),
    );
  }

  function listPendingDiffFiles(
    workspaceIds: string[],
    contents: Map<string, string>,
    sessionId?: string,
  ): PendingDiffFile[] {
    const seen = new Set<string>();
    const candidates: PendingDiffFile[] = [];
    for (const file of listSnapshotFiles(workspaceIds, sessionId)) {
      const key = fileKey(file.workspaceId, file.filePath);
      if (seen.has(key)) continue;
      seen.add(key);
      candidates.push(file);
    }
    for (const key of contents.keys()) {
      const parsed = parseFileKey(key);
      if (!parsed || (workspaceIds.length && !workspaceIds.includes(parsed.workspaceId))) continue;
      if (seen.has(key)) continue;
      seen.add(key);
      candidates.push(parsed);
    }
    return candidates
      .filter((file) => {
        const key = fileKey(file.workspaceId, file.filePath);
        if (dismissedMissing.value.has(key) || isMissingAccepted(file.workspaceId, file.filePath)) {
          return false;
        }
        if (missingFiles.value.has(key)) return true;
        const content = contents.get(key);
        if (content === undefined) return baselines.value.has(key);
        return getChunksForFile(file.workspaceId, file.filePath, content, sessionId).length > 0;
      })
      .sort((left, right) =>
        left.filePath.localeCompare(right.filePath) || left.workspaceId.localeCompare(right.workspaceId),
      );
  }

  function listMissingFiles(workspaceIds: string[]): PendingDiffFile[] {
    const allowed = new Set(workspaceIds);
    const files: PendingDiffFile[] = [];
    for (const key of missingFiles.value) {
      const parsed = parseFileKey(key);
      if (!parsed || (allowed.size && !allowed.has(parsed.workspaceId))) continue;
      files.push(parsed);
    }
    return files.sort((left, right) =>
      left.filePath.localeCompare(right.filePath) || left.workspaceId.localeCompare(right.workspaceId),
    );
  }

  function markFileMissing(workspaceId: string, filePath: string) {
    const key = fileKey(workspaceId, filePath);
    if (missingFiles.value.has(key)) return;
    const next = new Set(missingFiles.value);
    next.add(key);
    missingFiles.value = next;
    diskEpoch.value += 1;
  }

  function isMissingAccepted(workspaceId: string, filePath: string) {
    return dismissedMissing.value.has(fileKey(workspaceId, filePath));
  }

  async function acceptMissingFile(workspaceId: string, filePath: string) {
    const key = fileKey(workspaceId, filePath);
    const dismissed = new Set(dismissedMissing.value);
    dismissed.add(key);
    dismissedMissing.value = dismissed;
    markFilePresent(workspaceId, filePath);
    await removeBaseline(workspaceId, filePath);
  }

  function markFilePresent(workspaceId: string, filePath: string) {
    const key = fileKey(workspaceId, filePath);
    if (!missingFiles.value.has(key)) return;
    const next = new Set(missingFiles.value);
    next.delete(key);
    missingFiles.value = next;
  }

  async function readFileContent(workspaceId: string, filePath: string) {
    return backend.readWorkspaceFile(workspaceId, filePath).then(
      (file) => file.content,
      () => "",
    );
  }

  function putBaseline(workspaceId: string, filePath: string, content: string) {
    const key = fileKey(workspaceId, filePath);
    const next = new Map(baselines.value);
    next.set(key, content);
    baselines.value = next;
    return backend.saveFileBaseline(workspaceId, filePath, content);
  }

  function removeBaseline(workspaceId: string, filePath: string) {
    const key = fileKey(workspaceId, filePath);
    if (baselines.value.has(key)) {
      const next = new Map(baselines.value);
      next.delete(key);
      baselines.value = next;
    }
    reviewMemory.delete(key);
    return backend.deleteFileBaseline(workspaceId, filePath);
  }

  async function acceptChunk(chunkId: string, currentContent?: string) {
    const parsed = parseChunkId(chunkId);
    if (!parsed) return;
    await enqueueReview(parsed.workspaceId, parsed.filePath, async () => {
      const key = fileKey(parsed.workspaceId, parsed.filePath);
      const current = reviewMemory.get(key) ?? currentContent ?? await readFileContent(parsed.workspaceId, parsed.filePath);
      const baseline = baselineContent(parsed.workspaceId, parsed.filePath);
      if (baseline === undefined) return;
      const hunk = computeLineDiff(baseline, current).find(
        (item) => chunkKeyFor(item) === parsed.chunkKey,
      );
      if (!hunk) return;
      const nextBaseline = applyKeepHunk(baseline, hunk);
      if (nextBaseline === current) await removeBaseline(parsed.workspaceId, parsed.filePath);
      else await putBaseline(parsed.workspaceId, parsed.filePath, nextBaseline);
      diskEpoch.value += 1;
    });
  }

  async function undoChunk(
    chunkId: string,
    workspaceId: string,
    filePath: string,
    currentContent?: string,
  ) {
    const parsed = parseChunkId(chunkId);
    if (!parsed) return;
    await enqueueReview(workspaceId, filePath, async () => {
      const key = fileKey(workspaceId, filePath);
      const current = reviewMemory.get(key) ?? currentContent ?? await readFileContent(workspaceId, filePath);
      const baseline = baselineContent(workspaceId, filePath);
      if (baseline === undefined) return;
      const hunk = computeLineDiff(baseline, current).find(
        (item) => chunkKeyFor(item) === parsed.chunkKey,
      );
      if (!hunk) {
        console.warn(`[diff] chunk ${chunkId} 已不存在,取消 undo`);
        return;
      }
      const patched = applyReversePatch(current, hunk);
      const createdNewFile = hunk.oldLines.length === 0 && patched === "";
      if (createdNewFile) {
        await backend.deleteWorkspaceEntry(workspaceId, filePath).catch(() =>
          backend.writeWorkspaceFile(workspaceId, filePath, ""),
        );
        markFileMissing(workspaceId, filePath);
      } else {
        await backend.writeWorkspaceFile(workspaceId, filePath, patched);
        markFilePresent(workspaceId, filePath);
      }
      reviewMemory.set(key, patched);
      if (patched === baseline) await removeBaseline(workspaceId, filePath);
      diskEpoch.value += 1;
    });
  }

  async function acceptAllChunks(
    workspaceId: string,
    filePath: string,
    currentContent?: string,
  ) {
    await enqueueReview(workspaceId, filePath, async () => {
      const current = currentContent ?? await readFileContent(workspaceId, filePath);
      if (baselineContent(workspaceId, filePath) === undefined) return;
      if (baselineContent(workspaceId, filePath) === current) {
        await removeBaseline(workspaceId, filePath);
        return;
      }
      await removeBaseline(workspaceId, filePath);
      diskEpoch.value += 1;
    });
  }

  async function undoAllChunks(
    workspaceId: string,
    filePath: string,
    _currentContent?: string,
  ) {
    await enqueueReview(workspaceId, filePath, async () => {
      const baseline = baselineContent(workspaceId, filePath);
      if (baseline === undefined) return;
      if (baseline === "") {
        await backend.deleteWorkspaceEntry(workspaceId, filePath).catch(() =>
          backend.writeWorkspaceFile(workspaceId, filePath, ""),
        );
        markFileMissing(workspaceId, filePath);
      } else {
        await backend.writeWorkspaceFile(workspaceId, filePath, baseline);
        markFilePresent(workspaceId, filePath);
      }
      reviewMemory.set(fileKey(workspaceId, filePath), baseline);
      await removeBaseline(workspaceId, filePath);
      diskEpoch.value += 1;
    });
  }

  async function rollbackToTurn(
    messageId: string,
    workspaceId: string,
    sessionId: string,
  ) {
    const snapshotFiles = await backend.getTurnSnapshots(
      messageId,
      workspaceId,
    );

    for (const { filePath, content } of snapshotFiles) {
      if (content === "") {
        await backend.deleteWorkspaceEntry(workspaceId, filePath).catch(() =>
          backend.writeWorkspaceFile(workspaceId, filePath, ""),
        );
        markFileMissing(workspaceId, filePath);
      } else {
        await backend.writeWorkspaceFile(workspaceId, filePath, content);
        markFilePresent(workspaceId, filePath);
      }
      reviewMemory.delete(fileKey(workspaceId, filePath));
      if (baselineContent(workspaceId, filePath) === content) {
        await removeBaseline(workspaceId, filePath);
      }
    }

    const msgIds = turnOrder.value.get(sessionId) ?? [];
    const fromIndex = msgIds.indexOf(messageId);
    if (fromIndex >= 0) {
      const toRemove = new Set(msgIds.slice(fromIndex));

      const nextStates = new Map(chunkStates.value);
      for (const key of nextStates.keys()) {
        const parsed = parseChunkId(key);
        if (parsed && toRemove.has(parsed.messageId)) nextStates.delete(key);
      }
      chunkStates.value = nextStates;

      const nextSnapshots = new Map(snapshots.value);
      const sessionSnaps = nextSnapshots.get(sessionId);
      if (sessionSnaps) {
        msgIds.slice(fromIndex).forEach((id) => sessionSnaps.delete(id));
      }
      snapshots.value = nextSnapshots;
      const nextOrder = new Map(turnOrder.value);
      nextOrder.set(sessionId, msgIds.slice(0, fromIndex));
      turnOrder.value = nextOrder;
    }
    diskEpoch.value += 1;

    await backend.deleteSnapshotsFrom(messageId, workspaceId, sessionId);
  }

  function notifyDiskChanged() {
    reviewMemory.clear();
    diskEpoch.value += 1;
  }

  function onSnapshotReceived(
    messageId: string,
    filePath: string,
    content: string,
    sessionId: string,
    workspaceId = "",
  ) {
    onTurnSnapshotReady(sessionId, messageId, [{ workspaceId, filePath, content }]);
  }

  function onFileDeleted(filePath: string, workspaceId = "") {
    if (!filePath) return;
    markFileMissing(workspaceId, filePath);
  }

  function clearChunkStatesFrom(messageId: string, sessionId: string) {
    const order = turnOrder.value.get(sessionId) ?? [];
    const fromIndex = order.indexOf(messageId);
    const toRemove = new Set(fromIndex >= 0 ? order.slice(fromIndex) : [messageId]);
    const nextStates = new Map(chunkStates.value);
    for (const key of [...nextStates.keys()]) {
      const parsed = parseChunkId(key);
      if (parsed && toRemove.has(parsed.messageId)) nextStates.delete(key);
    }
    chunkStates.value = nextStates;
  }

  function onTurnSnapshotReady(
    sessionId: string,
    messageId: string,
    fileSnapshots: Array<{ workspaceId?: string; filePath: string; content: string }>,
  ) {
    const nextSnapshots = new Map(snapshots.value);
    const sessionSnaps = new Map(nextSnapshots.get(sessionId) ?? []);
    const fileMap = new Map(sessionSnaps.get(messageId) ?? []);
    for (const { workspaceId, filePath, content } of fileSnapshots) {
      if (!filePath) continue;
      fileMap.set(fileKey(workspaceId ?? "", filePath), content);
    }
    sessionSnaps.set(messageId, fileMap);
    nextSnapshots.set(sessionId, sessionSnaps);
    snapshots.value = nextSnapshots;
    const nextBaselines = new Map(baselines.value);
    let adopted = false;
    for (const { workspaceId, filePath, content } of fileSnapshots) {
      if (!filePath) continue;
      const key = fileKey(workspaceId ?? "", filePath);
      if (nextBaselines.has(key)) continue;
      nextBaselines.set(key, content);
      adopted = true;
    }
    if (adopted) baselines.value = nextBaselines;

    const order = turnOrder.value.get(sessionId) ?? [];
    if (!order.includes(messageId)) {
      const nextOrder = new Map(turnOrder.value);
      nextOrder.set(sessionId, [...order, messageId]);
      turnOrder.value = nextOrder;
    }
    diskEpoch.value += 1;
  }

  return {
    snapshots,
    baselines,
    chunkStates,
    turnOrder,
    diskEpoch,
    missingFiles,
    loadSession,
    loadSessions,
    loadActive,
    getChunksForFile,
    listSnapshotFiles,
    listPendingDiffFiles,
    listMissingFiles,
    markFileMissing,
    markFilePresent,
    isMissingAccepted,
    acceptMissingFile,
    acceptChunk,
    acceptAllChunks,
    undoChunk,
    undoAllChunks,
    rollbackToTurn,
    notifyDiskChanged,
    onSnapshotReceived,
    onFileDeleted,
    onTurnSnapshotReady,
    clearChunkStatesFrom,
  };
});
