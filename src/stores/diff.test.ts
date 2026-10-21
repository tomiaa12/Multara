import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { chunkKeyFor, makeChunkId } from "../domain/diff";
import type { SessionChunkState, SessionDiffData } from "../domain/diff";
import type { FileContent } from "../domain/types";
import { computeLineDiff } from "../utils/diff-engine";

const disk = new Map<string, string>();
const snapshotContents = new Map<string, string>();
const turnSnapshots = new Map<string, Array<{ filePath: string; content: string }>>();
const sessionData = new Map<string, SessionDiffData>();
const activeData: {
  snapshots: SessionDiffData["snapshots"];
  chunkStates: SessionChunkState[];
  baselines?: Array<{ workspaceId: string; filePath: string; content: string }>;
} = {
  snapshots: [],
  chunkStates: [],
};
const deletedEntries: string[] = [];
const deletedFrom: string[] = [];

function diskKey(workspaceId: string, filePath: string) {
  return `${workspaceId}\n${filePath}`;
}

function snapKey(messageId: string, workspaceId: string, filePath: string) {
  return `${messageId}\n${workspaceId}\n${filePath}`;
}

vi.mock("../services/backend", () => ({
  loadSessionDiffData: vi.fn(async (sessionId: string) => {
    return (
      sessionData.get(sessionId) ?? {
        snapshots: [],
        chunkStates: [],
        messageIds: [],
      }
    );
  }),
  loadActiveDiffData: vi.fn(async () => ({
    snapshots: activeData.snapshots.map((item) => ({ ...item, sessionId: "s1" })),
    chunkStates: activeData.chunkStates,
    baselines: activeData.baselines,
  })),
  getSnapshotContent: vi.fn(async (messageId: string, workspaceId: string, filePath: string) => {
    return snapshotContents.get(snapKey(messageId, workspaceId, filePath)) ?? "";
  }),
  getTurnSnapshots: vi.fn(async (messageId: string, workspaceId: string) => {
    return turnSnapshots.get(`${messageId}\n${workspaceId}`) ?? [];
  }),
  updateChunkState: vi.fn(async () => undefined),
  writeWorkspaceFile: vi.fn(async (workspaceId: string, path: string, content: string) => {
    disk.set(diskKey(workspaceId, path), content);
  }),
  readWorkspaceFile: vi.fn(async (workspaceId: string, path: string) => {
    const content = disk.get(diskKey(workspaceId, path));
    if (content === undefined) throw new Error("missing");
    return {
      workspaceId,
      path,
      content,
      language: "ts",
      size: content.length,
      readOnly: false,
    } satisfies FileContent;
  }),
  deleteWorkspaceEntry: vi.fn(async (workspaceId: string, path: string) => {
    disk.delete(diskKey(workspaceId, path));
    deletedEntries.push(path);
  }),
  deleteSnapshotsFrom: vi.fn(async (messageId: string) => {
    deletedFrom.push(messageId);
  }),
  saveFileBaseline: vi.fn(async () => undefined),
  deleteFileBaseline: vi.fn(async () => undefined),
}));

import { useDiffStore } from "./diff";

function idOf(
  messageId: string,
  filePath: string,
  baseline: string,
  current: string,
  index = 0,
) {
  const hunk = computeLineDiff(baseline, current)[index];
  return makeChunkId(messageId, "ws", filePath, chunkKeyFor(hunk));
}

function seedSession(
  sessionId: string,
  turns: Array<{ messageId: string; files: Record<string, string> }>,
  states: SessionChunkState[] = [],
) {
  const snapshots = turns.flatMap((turn) =>
    Object.entries(turn.files).map(([filePath, content]) => {
      snapshotContents.set(snapKey(turn.messageId, "ws", filePath), content);
      return { messageId: turn.messageId, workspaceId: "ws", filePath, content };
    }),
  );
  for (const turn of turns) {
    turnSnapshots.set(
      `${turn.messageId}\nws`,
      Object.entries(turn.files).map(([filePath, content]) => ({ filePath, content })),
    );
  }
  sessionData.set(sessionId, {
    snapshots,
    chunkStates: states,
    messageIds: turns.map((turn) => turn.messageId),
  });
}

describe("diffStore", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    disk.clear();
    snapshotContents.clear();
    turnSnapshots.clear();
    sessionData.clear();
    activeData.snapshots = [];
    activeData.chunkStates = [];
    activeData.baselines = undefined;
    deletedEntries.length = 0;
    deletedFrom.length = 0;
  });

  test("T-A01 accept hides one chunk and leaves the other pending", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A\nB\nC\nD\nE" } }]);
    disk.set(diskKey("ws", "a.ts"), "A\nX\nC\nD\nY");
    const store = useDiffStore();
    await store.loadSession("s1");
    const current = "A\nX\nC\nD\nY";
    const chunk1 = idOf("m1", "a.ts", "A\nB\nC\nD\nE", current, 1);
    await store.acceptChunk(chunk1);

    expect(disk.get(diskKey("ws", "a.ts"))).toBe(current);
    const chunks = store.getChunksForFile("ws", "a.ts", current);
    expect(chunks).toHaveLength(1);
    expect(chunks[0].hunk).toMatchObject({ oldLines: ["B"], newLines: ["X"] });
  });

  test("accept all pending chunks of the open file", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A\nB\nC\nD\nE" } }]);
    const current = "A\nX\nC\nD\nY";
    disk.set(diskKey("ws", "a.ts"), current);
    const store = useDiffStore();
    await store.loadSession("s1");

    await store.acceptAllChunks("ws", "a.ts", current);

    expect(store.getChunksForFile("ws", "a.ts", current)).toEqual([]);
    expect(disk.get(diskKey("ws", "a.ts"))).toBe(current);
  });

  test("undo all pending chunks of the open file restores the baseline", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A\nB\nC\nD\nE" } }]);
    disk.set(diskKey("ws", "a.ts"), "A\nX\nC\nD\nY");
    const store = useDiffStore();
    await store.loadSession("s1");

    await store.undoAllChunks("ws", "a.ts", "A\nX\nC\nD\nY");

    expect(disk.get(diskKey("ws", "a.ts"))).toBe("A\nB\nC\nD\nE");
    expect(store.getChunksForFile("ws", "a.ts", "A\nB\nC\nD\nE")).toEqual([]);
  });

  test("T-A02 accept all chunks hides the file diff", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A\nB\nC\nD\nE" } }]);
    disk.set(diskKey("ws", "a.ts"), "A\nX\nC\nD\nY");
    const store = useDiffStore();
    await store.loadSession("s1");
    const current = "A\nX\nC\nD\nY";
    await store.acceptChunk(idOf("m1", "a.ts", "A\nB\nC\nD\nE", current, 0));
    await store.acceptChunk(idOf("m1", "a.ts", "A\nB\nC\nD\nE", current, 1));
    expect(store.getChunksForFile("ws", "a.ts", current)).toEqual([]);
    expect(disk.get(diskKey("ws", "a.ts"))).toBe(current);
  });

  test("T-U01 undo restores a pure insert", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "line1\nline2\nline3" } }]);
    disk.set(diskKey("ws", "a.ts"), "line1\nNEW\nline2\nline3");
    const store = useDiffStore();
    await store.loadSession("s1");
    const chunkId = idOf("m1", "a.ts", "line1\nline2\nline3", "line1\nNEW\nline2\nline3");
    await store.undoChunk(chunkId, "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("line1\nline2\nline3");
    expect(store.getChunksForFile("ws", "a.ts", disk.get(diskKey("ws", "a.ts"))!)).toEqual([]);
  });

  test("T-U02 undo restores a deleted line", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A\nB\nC" } }]);
    disk.set(diskKey("ws", "a.ts"), "A\nC");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.undoChunk(idOf("m1", "a.ts", "A\nB\nC", "A\nC"), "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("A\nB\nC");
  });

  test("T-U03 undo restores a replaced line", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "const x = 1" } }]);
    disk.set(diskKey("ws", "a.ts"), "const x = 999");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.undoChunk(idOf("m1", "a.ts", "const x = 1", "const x = 999"), "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("const x = 1");
  });

  test("T-U04 undo one chunk leaves the other on disk", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A\nB\nC\nD\nE" } }]);
    disk.set(diskKey("ws", "a.ts"), "A\nX\nC\nD\nY");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.undoChunk(idOf("m1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 0), "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("A\nB\nC\nD\nY");
    const leftover = store.getChunksForFile("ws", "a.ts", "A\nB\nC\nD\nY");
    expect(leftover).toHaveLength(1);
    expect(leftover[0].hunk).toMatchObject({ oldLines: ["E"], newLines: ["Y"] });
  });

  test("T-A03 undo a pure insert removes the new line", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "line1\nline2" } }]);
    disk.set(diskKey("ws", "a.ts"), "line1\nNEW\nline2");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.undoChunk(idOf("m1", "a.ts", "line1\nline2", "line1\nNEW\nline2"), "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("line1\nline2");
    expect(store.getChunksForFile("ws", "a.ts", "line1\nline2")).toEqual([]);
  });

  test("T-A05 undo of a new file deletes the file", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "src/new.ts": "" } }]);
    disk.set(diskKey("ws", "src/new.ts"), "export default {}");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.undoChunk(idOf("m1", "src/new.ts", "", "export default {}"), "ws", "src/new.ts");
    expect(deletedEntries).toContain("src/new.ts");
    expect(disk.has(diskKey("ws", "src/new.ts"))).toBe(false);
  });

  test("T-E01 undo works for a one-line file", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "only line" } }]);
    disk.set(diskKey("ws", "a.ts"), "replaced");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.undoChunk(idOf("m1", "a.ts", "only line", "replaced"), "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("only line");
  });

  test("T-E02 second undo is idempotent", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "line1\nline2\nline3" } }]);
    disk.set(diskKey("ws", "a.ts"), "line1\nNEW\nline2\nline3");
    const store = useDiffStore();
    await store.loadSession("s1");
    const chunkId = idOf("m1", "a.ts", "line1\nline2\nline3", "line1\nNEW\nline2\nline3");
    await store.undoChunk(chunkId, "ws", "a.ts");
    await store.undoChunk(chunkId, "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("line1\nline2\nline3");
    expect(store.getChunksForFile("ws", "a.ts", "line1\nline2\nline3")).toEqual([]);
    warn.mockRestore();
  });

  test("T-E03 accepted chunk can still be force-undone", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "const x = 1" } }]);
    disk.set(diskKey("ws", "a.ts"), "const x = 999");
    const store = useDiffStore();
    await store.loadSession("s1");
    const chunkId = idOf("m1", "a.ts", "const x = 1", "const x = 999");
    await store.acceptChunk(chunkId);
    await store.undoChunk(chunkId, "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("const x = 999");
    expect(store.getChunksForFile("ws", "a.ts", "const x = 999")).toEqual([]);
  });

  test("T-R01 rollback turn 2 restores b.ts and keeps accepted a.ts hidden", async () => {
    seedSession(
      "s1",
      [
        { messageId: "t1", files: { "a.ts": "B", "b.ts": "D" } },
        { messageId: "t2", files: { "a.ts": "X", "b.ts": "D" } },
      ],
      [{ chunkId: idOf("t1", "a.ts", "B", "X"), status: "accepted" }],
    );
    disk.set(diskKey("ws", "a.ts"), "X");
    disk.set(diskKey("ws", "b.ts"), "Y");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.acceptAllChunks("ws", "a.ts", "X");
    await store.rollbackToTurn("t2", "ws", "s1");

    expect(disk.get(diskKey("ws", "a.ts"))).toBe("X");
    expect(disk.get(diskKey("ws", "b.ts"))).toBe("D");
    expect(store.turnOrder.get("s1")).toEqual(["t1"]);
    expect(store.chunkStates.has(makeChunkId("t2", "ws", "b.ts", "gone"))).toBe(false);
    expect(store.getChunksForFile("ws", "a.ts", "X")).toEqual([]);
    expect(store.getChunksForFile("ws", "b.ts", "D")).toEqual([]);
    expect(deletedFrom).toContain("t2");
  });

  test("T-R02 rollback turn 1 restores a clean workspace", async () => {
    seedSession("s1", [{ messageId: "t1", files: { "a.ts": "ORIGINAL" } }]);
    disk.set(diskKey("ws", "a.ts"), "MODIFIED");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.rollbackToTurn("t1", "ws", "s1");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("ORIGINAL");
    expect(store.turnOrder.get("s1")).toEqual([]);
    expect(store.getChunksForFile("ws", "a.ts", "ORIGINAL")).toEqual([]);
  });

  test("T-R03 rollback keeps earlier accepted/pending mix", async () => {
    seedSession(
      "s1",
      [
        { messageId: "t1", files: { "a.ts": "A\nB\nC\nD\nE" } },
        { messageId: "t2", files: { "a.ts": "A\nX\nC\nD\nY", "b.ts": "D" } },
      ],
      [
        { chunkId: idOf("t1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 0), status: "pending" },
        { chunkId: idOf("t1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 1), status: "accepted" },
      ],
    );
    disk.set(diskKey("ws", "a.ts"), "A\nX\nC\nD\nY");
    disk.set(diskKey("ws", "b.ts"), "Y");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.acceptChunk(idOf("t1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 1));
    await store.rollbackToTurn("t2", "ws", "s1");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("A\nX\nC\nD\nY");
    expect(disk.get(diskKey("ws", "b.ts"))).toBe("D");
    const chunks = store.getChunksForFile("ws", "a.ts", "A\nX\nC\nD\nY");
    expect(chunks).toHaveLength(1);
    expect(chunks[0].hunk).toMatchObject({ oldLines: ["B"], newLines: ["X"] });
  });

  test("T-R04 rollback to turn 2 drops later turns from store", async () => {
    seedSession("s1", [
      { messageId: "t1", files: { "a.ts": "v1" } },
      { messageId: "t2", files: { "a.ts": "v2" } },
      { messageId: "t3", files: { "a.ts": "v3" } },
      { messageId: "t4", files: { "a.ts": "v4" } },
    ]);
    const store = useDiffStore();
    await store.loadSession("s1");
    store.chunkStates.set(makeChunkId("t1", "ws", "a.ts", "keep"), "pending");
    store.chunkStates.set(makeChunkId("t3", "ws", "a.ts", "drop"), "pending");
    await store.rollbackToTurn("t2", "ws", "s1");
    expect(store.turnOrder.get("s1")).toEqual(["t1"]);
    expect([...store.chunkStates.keys()]).toEqual([makeChunkId("t1", "ws", "a.ts", "keep")]);
  });

  test("T-R05 new snapshot after rollback uses rolled-back disk", async () => {
    seedSession("s1", [
      { messageId: "t1", files: { "a.ts": "v1" } },
      { messageId: "t2", files: { "a.ts": "v2" } },
    ]);
    disk.set(diskKey("ws", "a.ts"), "v2");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.rollbackToTurn("t2", "ws", "s1");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("v2");
    store.onTurnSnapshotReady("s1", "t2-new", [{ workspaceId: "ws", filePath: "a.ts", content: disk.get(diskKey("ws", "a.ts"))! }]);
    expect(store.turnOrder.get("s1")).toEqual(["t1", "t2-new"]);
    expect(store.getChunksForFile("ws", "a.ts", "v2").every((chunk) => chunk.messageId !== "t2-new")).toBe(true);
  });

  test("T-R03 keep-code clears later chunk states and keeps earlier pending", async () => {
    seedSession(
      "s1",
      [
        { messageId: "t1", files: { "a.ts": "v0" } },
        { messageId: "t2", files: { "b.ts": "u0" } },
      ],
      [
        { chunkId: idOf("t1", "a.ts", "v0", "v1"), status: "pending" },
        { chunkId: idOf("t2", "b.ts", "u0", "u1"), status: "pending" },
      ],
    );
    disk.set(diskKey("ws", "a.ts"), "v1");
    disk.set(diskKey("ws", "b.ts"), "u1");
    const store = useDiffStore();
    await store.loadSession("s1");
    store.clearChunkStatesFrom("t2", "s1");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("v1");
    expect(disk.get(diskKey("ws", "b.ts"))).toBe("u1");
    expect(store.chunkStates.has(idOf("t2", "b.ts", "u0", "u1"))).toBe(false);
    expect(store.getChunksForFile("ws", "a.ts", "v1")).toHaveLength(1);
  });

  test("rollback of a turn without snapshot keeps earlier baselines", async () => {
    seedSession("s1", [{ messageId: "t1", files: { "a.ts": "COMMENTED" } }]);
    disk.set(diskKey("ws", "a.ts"), "CLEAN");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.rollbackToTurn("t2-missing", "ws", "s1");
    expect(store.turnOrder.get("s1")).toEqual(["t1"]);
    expect(store.getChunksForFile("ws", "a.ts", "CLEAN")[0]?.hunk.oldLines).toEqual(["COMMENTED"]);
  });

  test("T-R01 rollback turn 2 restores b.ts and keeps turn 1 pending", async () => {
    seedSession("s1", [
      { messageId: "msg1", files: { "a.ts": "v0" } },
      { messageId: "msg2", files: { "b.ts": "u0" } },
    ]);
    disk.set(diskKey("ws", "a.ts"), "v1");
    disk.set(diskKey("ws", "b.ts"), "u1");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.rollbackToTurn("msg2", "ws", "s1");
    expect(disk.get(diskKey("ws", "b.ts"))).toBe("u0");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("v1");
    expect(store.getChunksForFile("ws", "a.ts", "v1")).toHaveLength(1);
    expect(store.turnOrder.get("s1")).toEqual(["msg1"]);
  });

  test("T-S05 onSnapshotReceived caches a live baseline", () => {
    const store = useDiffStore();
    store.onSnapshotReceived("m1", "a.ts", "v0", "s1", "ws");
    expect(store.turnOrder.get("s1")).toEqual(["m1"]);
    expect(store.getChunksForFile("ws", "a.ts", "v1", "s1")[0].hunk).toMatchObject({
      oldLines: ["v0"],
      newLines: ["v1"],
    });
  });

  test("T-A06 accept after undo is idempotent", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "line1\nOLD\nline3" } }]);
    disk.set(diskKey("ws", "a.ts"), "line1\nNEW\nline3");
    const store = useDiffStore();
    await store.loadSession("s1");
    const chunkId = idOf("m1", "a.ts", "line1\nOLD\nline3", "line1\nNEW\nline3");
    await store.undoChunk(chunkId, "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("line1\nOLD\nline3");
    await store.acceptChunk(chunkId);
    expect(store.getChunksForFile("ws", "a.ts", "line1\nOLD\nline3")).toEqual([]);
  });

  test("T-L01 live edits expand the same pending hunk then undo to baseline", async () => {
    const store = useDiffStore();
    store.onTurnSnapshotReady("s1", "m1", [{ workspaceId: "ws", filePath: "a.ts", content: "A\nB\nC" }]);
    disk.set(diskKey("ws", "a.ts"), "A\nX\nC");
    const first = store.getChunksForFile("ws", "a.ts", "A\nX\nC", "s1");
    expect(first[0].hunk).toMatchObject({ oldLines: ["B"], newLines: ["X"] });
    disk.set(diskKey("ws", "a.ts"), "A\nX\nY\nC");
    const second = store.getChunksForFile("ws", "a.ts", "A\nX\nY\nC", "s1");
    expect(second[0].hunk).toMatchObject({ oldLines: ["B"], newLines: ["X", "Y"] });
    snapshotContents.set(snapKey("m1", "ws", "a.ts"), "A\nB\nC");
    await store.undoChunk(second[0].chunkId, "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("A\nB\nC");
  });

  test("T-E04 keeps file-unique chunks when switching to an empty session", async () => {
    seedSession("sessA", [{ messageId: "a1", files: { "a.ts": "v0" } }]);
    seedSession("sessB", []);
    disk.set(diskKey("ws", "a.ts"), "v1");
    const store = useDiffStore();
    await store.loadSessions(["sessA", "sessB"]);
    expect(store.getChunksForFile("ws", "a.ts", "v1", "sessB")).toHaveLength(1);
    expect(store.getChunksForFile("ws", "a.ts", "v1", "sessA")).toHaveLength(1);
    expect(store.getChunksForFile("ws", "a.ts", "v1")).toHaveLength(1);
    expect(store.listPendingDiffFiles(["ws"], new Map([[diskKey("ws", "a.ts"), "v1"]]))).toEqual([
      { workspaceId: "ws", filePath: "a.ts" },
    ]);
  });

  test("loadActive only hydrates unresolved file baselines", async () => {
    activeData.snapshots = [{ messageId: "a1", workspaceId: "ws", filePath: "a.ts", content: "v0" }];
    disk.set(diskKey("ws", "a.ts"), "v1");
    disk.set(diskKey("ws", "b.ts"), "ignored");
    const store = useDiffStore();
    await store.loadActive();
    expect(store.getChunksForFile("ws", "a.ts", "v1")).toHaveLength(1);
    expect(store.getChunksForFile("ws", "b.ts", "ignored")).toEqual([]);
    expect(store.listSnapshotFiles(["ws"]).map((file) => file.filePath)).toEqual(["a.ts"]);
  });

  test("T-restart loadSession restores snapshots and chunk states", async () => {
    seedSession(
      "s1",
      [{ messageId: "t1", files: { "a.ts": "v0" } }],
      [{ chunkId: idOf("t1", "a.ts", "v0", "v1"), status: "accepted" }],
    );
    disk.set(diskKey("ws", "a.ts"), "v1");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.acceptAllChunks("ws", "a.ts", "v1");
    expect(store.getChunksForFile("ws", "a.ts", "v1")).toEqual([]);
    activeData.baselines = [];
    activeData.snapshots = [{ messageId: "t1", workspaceId: "ws", filePath: "a.ts", content: "v0" }];
    await store.loadActive();
    expect(store.getChunksForFile("ws", "a.ts", "v1")).toEqual([]);
  });

  test("lists only files that still have Accept/Undo chunks", async () => {
    seedSession("s1", [
      { messageId: "m1", files: { "a.ts": "v0", "b.ts": "keep", "c.ts": "old" } },
    ]);
    const store = useDiffStore();
    await store.loadSession("s1");
    const files = store.listPendingDiffFiles(
      ["ws"],
      new Map([
        [diskKey("ws", "a.ts"), "v1"],
        [diskKey("ws", "b.ts"), "keep"],
        [diskKey("ws", "c.ts"), "new"],
      ]),
    );
    expect(files.map((file) => file.filePath)).toEqual(["a.ts", "c.ts"]);
    await store.acceptChunk(idOf("m1", "a.ts", "v0", "v1"), "v1");
    expect(
      store.listPendingDiffFiles(
        ["ws"],
        new Map([
          [diskKey("ws", "a.ts"), "v1"],
          [diskKey("ws", "b.ts"), "keep"],
          [diskKey("ws", "c.ts"), "new"],
        ]),
      ).map((file) => file.filePath),
    ).toEqual(["c.ts"]);
  });

  test("missing file with an empty baseline stays pending until accepted", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "internal/common/mapping/client_proof_test.go": "" } }]);
    const store = useDiffStore();
    await store.loadSession("s1");
    store.markFileMissing("ws", "internal/common/mapping/client_proof_test.go");
    const key = diskKey("ws", "internal/common/mapping/client_proof_test.go");
    const contents = new Map([[key, ""]]);

    expect(store.listPendingDiffFiles(["ws"], contents).map((file) => file.filePath)).toEqual([
      "internal/common/mapping/client_proof_test.go",
    ]);
    expect(store.getChunksForFile("ws", "internal/common/mapping/client_proof_test.go", "")).toEqual([]);

    await store.acceptMissingFile("ws", "internal/common/mapping/client_proof_test.go");
    store.markFileMissing("ws", "internal/common/mapping/client_proof_test.go");

    expect(store.isMissingAccepted("ws", "internal/common/mapping/client_proof_test.go")).toBe(true);
    expect(store.listPendingDiffFiles(["ws"], contents)).toEqual([]);
  });

  test("T-E10 stored chunk status is unchanged after a header edit", async () => {
    seedSession("s1", [{ messageId: "t1", files: { "a.ts": "A\nB\nC\nD\nE" } }]);
    const store = useDiffStore();
    await store.loadSession("s1");
    const current = "A\nX\nC\nD\nY";
    await store.acceptChunk(idOf("t1", "a.ts", "A\nB\nC\nD\nE", current, 0), current);
    expect(store.getChunksForFile("ws", "a.ts", current)).toHaveLength(1);
    store.notifyDiskChanged();
    expect(store.getChunksForFile("ws", "a.ts", current)).toHaveLength(1);
    expect(store.getChunksForFile("ws", "a.ts", current)[0].hunk.oldLines).toEqual(["E"]);
  });

  test("P0-1 undo 使用稳定 hunk 而不是数组下标，避免误删用户插入", async () => {
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A\nB\nC" } }]);
    disk.set(diskKey("ws", "a.ts"), "USER\nA\nX\nC");
    const store = useDiffStore();
    await store.loadSession("s1");
    const chunks = store.getChunksForFile("ws", "a.ts", "USER\nA\nX\nC");
    const aiChunk = chunks.find((chunk) => chunk.hunk.oldLines.includes("B"));
    expect(aiChunk).toBeTruthy();
    await store.undoChunk(aiChunk!.chunkId, "ws", "a.ts");
    expect(disk.get(diskKey("ws", "a.ts"))).toBe("USER\nA\nB\nC");
  });

  test("P0-1 找不到 hunk 时不写入 undone", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    seedSession("s1", [{ messageId: "m1", files: { "a.ts": "A" } }]);
    disk.set(diskKey("ws", "a.ts"), "A");
    const store = useDiffStore();
    await store.loadSession("s1");
    await store.undoChunk(makeChunkId("m1", "ws", "a.ts", "missing"), "ws", "a.ts");
    expect(store.chunkStates.has(makeChunkId("m1", "ws", "a.ts", "missing"))).toBe(false);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });
});
