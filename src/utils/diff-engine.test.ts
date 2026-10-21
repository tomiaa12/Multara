import { describe, expect, test } from "vitest";
import { chunkKeyFor, makeChunkId } from "../domain/diff";
import {
  applyReversePatch,
  computeLineDiff,
  getActiveChunksForFile,
} from "./diff-engine";

function idOf(
  messageId: string,
  filePath: string,
  baseline: string,
  current: string,
  index = 0,
  workspaceId = "ws",
) {
  return makeChunkId(
    messageId,
    workspaceId,
    filePath,
    chunkKeyFor(computeLineDiff(baseline, current)[index]),
  );
}

describe("T-D01 纯新增行", () => {
  test("inserts two lines as a single green hunk", () => {
    const [hunk] = computeLineDiff("line1\nline2", "line1\nNEW_LINE_A\nNEW_LINE_B\nline2");
    expect(hunk).toMatchObject({
      oldStart: 2,
      oldLines: [],
      newStart: 2,
      newLines: ["NEW_LINE_A", "NEW_LINE_B"],
    });
  });
});

describe("T-D02 纯删除行", () => {
  test("marks the removed line as red-only", () => {
    const [hunk] = computeLineDiff("line1\nDELETE_ME\nline2", "line1\nline2");
    expect(hunk).toMatchObject({
      oldStart: 2,
      oldLines: ["DELETE_ME"],
      newStart: 2,
      newLines: [],
    });
  });
});

describe("T-D03 替换行", () => {
  test("replaces a single line", () => {
    const [hunk] = computeLineDiff(
      "line1\nOLD_CONTENT\nline3",
      "line1\nNEW_CONTENT\nline3",
    );
    expect(hunk).toMatchObject({
      oldStart: 2,
      oldLines: ["OLD_CONTENT"],
      newStart: 2,
      newLines: ["NEW_CONTENT"],
    });
  });
});

describe("T-D04 多段不连续变更", () => {
  test("returns two independent hunks", () => {
    const hunks = computeLineDiff("A\nB\nC\nD\nE", "A\nB_MODIFIED\nC\nD\nE_MODIFIED");
    expect(hunks).toHaveLength(2);
    expect(hunks[0]).toMatchObject({
      oldLines: ["B"],
      newLines: ["B_MODIFIED"],
      newStart: 2,
    });
    expect(hunks[1]).toMatchObject({
      oldLines: ["E"],
      newLines: ["E_MODIFIED"],
      newStart: 5,
    });
  });
});

describe("T-D05 空文件 → 有内容", () => {
  test("treats the whole file as added", () => {
    const [hunk] = computeLineDiff("", "line1\nline2");
    expect(hunk).toMatchObject({
      oldStart: 1,
      oldLines: [],
      newStart: 1,
      newLines: ["line1", "line2"],
    });
  });
});

describe("T-D06 有内容 → 空文件", () => {
  test("treats the whole file as deleted", () => {
    const [hunk] = computeLineDiff("line1\nline2", "");
    expect(hunk).toMatchObject({
      oldStart: 1,
      oldLines: ["line1", "line2"],
      newStart: 1,
      newLines: [],
    });
  });
});

describe("T-D05 AI 仅新增一行空行（spec）", () => {
  test("keeps the closing brace out of the hunk", () => {
    const [hunk] = computeLineDiff(
      "function foo() {\n  return 1\n}",
      "function foo() {\n  return 1\n\n}",
    );
    expect(hunk).toMatchObject({
      oldLines: [],
      newLines: [""],
      newStart: 3,
    });
  });
});

describe("T-D07 AI 仅新增一行空行", () => {
  test("only the blank line is green", () => {
    const baseline = "function foo() {\n    return 1\n  }";
    const current = "function foo() {\n    return 1\n\n  }";
    const [hunk] = computeLineDiff(baseline, current);
    expect(hunk).toMatchObject({
      oldStart: 3,
      oldLines: [],
      newStart: 3,
      newLines: [""],
    });
  });
});

describe("T-D08 内容完全相同", () => {
  test("returns an empty array", () => {
    expect(computeLineDiff("line1\nline2\nline3", "line1\nline2\nline3")).toEqual([]);
  });
});

describe("T-D09 Unicode 内容", () => {
  test("diffs CJK text as a normal replacement", () => {
    const [hunk] = computeLineDiff('const msg = "Hello"', 'const msg = "你好，世界"');
    expect(hunk).toMatchObject({
      oldLines: ['const msg = "Hello"'],
      newLines: ['const msg = "你好，世界"'],
    });
  });
});

describe("T-D10 反向 Patch —— 还原修改行", () => {
  test("restores the replaced line", () => {
    const current = "line1\nNEW_CONTENT\nline3";
    const [hunk] = computeLineDiff("line1\nOLD_CONTENT\nline3", current);
    expect(applyReversePatch(current, hunk)).toBe("line1\nOLD_CONTENT\nline3");
  });
});

describe("T-D11 反向 Patch —— 还原新增行", () => {
  test("removes the inserted line", () => {
    const current = "line1\nEXTRA_LINE\nline2";
    const [hunk] = computeLineDiff("line1\nline2", current);
    expect(hunk).toMatchObject({ newStart: 2, newLines: ["EXTRA_LINE"], oldLines: [] });
    expect(applyReversePatch(current, hunk)).toBe("line1\nline2");
  });
});

describe("T-D12 反向 Patch —— 还原删除行", () => {
  test("reinserts the deleted line", () => {
    const current = "line1\nline3";
    const [hunk] = computeLineDiff("line1\nline2\nline3", current);
    expect(hunk).toMatchObject({ newStart: 2, newLines: [], oldLines: ["line2"] });
    expect(applyReversePatch(current, hunk)).toBe("line1\nline2\nline3");
  });
});

describe("T-D13 Chunk 中间有手动编辑", () => {
  test("folds the manual line into the green hunk", () => {
    const [hunk] = computeLineDiff("A\nB\nC", "A\nX\nMANUAL\nC");
    expect(hunk).toMatchObject({
      oldLines: ["B"],
      newLines: ["X", "MANUAL"],
    });
  });
});

describe("T-D14 在绿色区域最后一行按回车", () => {
  test("keeps the blank line inside the green hunk", () => {
    const [hunk] = computeLineDiff("A\nB\nC", "A\nX\nY\n\nC");
    expect(hunk).toMatchObject({
      oldLines: ["B"],
      newLines: ["X", "Y", ""],
    });
  });
});

describe("T-D02 重复删除去重", () => {
  test("two turns with the same deletion only keep one visual chunk", () => {
    const baseline = "29\n# 编译后文件不上传到 git\nlib";
    const current = "29\nlib";
    expect(computeLineDiff(baseline, current)).toHaveLength(1);
    expect(computeLineDiff(baseline, current)[0]).toMatchObject({
      oldLines: ["# 编译后文件不上传到 git"],
      newLines: [],
    });
    const chunks = getActiveChunksForFile(
      "ws",
      ".gitignore",
      current,
      [
        { messageId: "ask1", workspaceId: "ws", baselineContent: baseline },
        { messageId: "ask2", workspaceId: "ws", baselineContent: baseline },
      ],
      new Map(),
    );
    expect(chunks).toHaveLength(1);
    expect(chunks[0].hunk.oldLines).toEqual(["# 编译后文件不上传到 git"]);
  });
});

describe("getActiveChunksForFile", () => {
  test("T-A01 hides accepted chunks and keeps pending ones", () => {
    const chunkStates = new Map<"pending" | "accepted" | "undone" | string, "pending" | "accepted" | "undone">([
      [idOf("m1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 1), "accepted"],
    ]);
    const chunks = getActiveChunksForFile(
      "ws",
      "a.ts",
      "A\nX\nC\nD\nY",
      [{ messageId: "m1", workspaceId: "ws", baselineContent: "A\nB\nC\nD\nE" }],
      chunkStates,
    );
    expect(chunks).toHaveLength(1);
    expect(chunks[0].chunkId).toBe(idOf("m1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 0));
    expect(chunks[0].hunk).toMatchObject({ oldLines: ["B"], newLines: ["X"] });
  });

  test("T-A02 hides every chunk after all are accepted", () => {
    const chunkStates = new Map([
      [idOf("m1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 0), "accepted" as const],
      [idOf("m1", "a.ts", "A\nB\nC\nD\nE", "A\nX\nC\nD\nY", 1), "accepted" as const],
    ]);
    expect(
      getActiveChunksForFile(
        "ws",
        "a.ts",
        "A\nX\nC\nD\nY",
        [{ messageId: "m1", workspaceId: "ws", baselineContent: "A\nB\nC\nD\nE" }],
        chunkStates,
      ),
    ).toEqual([]);
  });

  test("T-A03 accepted hunk stays hidden; editing new lines creates a new pending identity", () => {
    const original = idOf("m1", "a.ts", "A\nB\nC\nD", "A\nX\nC\nY");
    const chunkStates = new Map([[original, "accepted" as const]]);
    const chunks = getActiveChunksForFile(
      "ws",
      "a.ts",
      "A\nX // edited\nC\nY",
      [{ messageId: "m1", workspaceId: "ws", baselineContent: "A\nB\nC\nD" }],
      chunkStates,
    );
    expect(chunks.some((chunk) => chunk.hunk.newLines[0] === "X // edited")).toBe(true);
    expect(chunks.some((chunk) => chunk.hunk.oldLines[0] === "D")).toBe(true);
  });

  test("T-S05 empty baseline shows a new file as all green", () => {
    const chunks = getActiveChunksForFile(
      "ws",
      "src/new-feature.ts",
      "export default {}\n",
      [{ messageId: "m1", workspaceId: "ws", baselineContent: "" }],
      new Map(),
    );
    expect(chunks).toHaveLength(1);
    expect(chunks[0].hunk.oldLines).toEqual([]);
    expect(chunks[0].hunk.newLines.join("\n")).toContain("export default {}");
  });

  test("P1-5 unmatched insert hunks are assigned their own content hash", () => {
    const chunks = getActiveChunksForFile(
      "ws",
      "a.ts",
      "A\nUSER\nB",
      [{ messageId: "m1", workspaceId: "ws", baselineContent: "A\nB" }],
      new Map(),
    );
    expect(chunks).toHaveLength(1);
    expect(chunks[0].hunk).toMatchObject({ oldLines: [], newLines: ["USER"] });
  });
});

describe("T-U 反向还原", () => {
  test("T-U01 undoes a pure insert", () => {
    const current = "line1\nNEW\nline2\nline3";
    const [hunk] = computeLineDiff("line1\nline2\nline3", current);
    const restored = applyReversePatch(current, hunk);
    expect(restored).toBe("line1\nline2\nline3");
    expect(computeLineDiff("line1\nline2\nline3", restored)).toEqual([]);
  });

  test("T-U02 undoes a pure delete", () => {
    const current = "A\nC";
    const [hunk] = computeLineDiff("A\nB\nC", current);
    expect(applyReversePatch(current, hunk)).toBe("A\nB\nC");
  });

  test("T-U03 undoes a replacement", () => {
    const current = "const x = 999";
    const [hunk] = computeLineDiff("const x = 1", current);
    expect(applyReversePatch(current, hunk)).toBe("const x = 1");
  });

  test("T-U04 undoing one chunk leaves the other", () => {
    const baseline = "A\nB\nC\nD\nE";
    const current = "A\nX\nC\nD\nY";
    const hunks = computeLineDiff(baseline, current);
    const restored = applyReversePatch(current, hunks[0]);
    expect(restored).toBe("A\nB\nC\nD\nY");
    const leftover = computeLineDiff(baseline, restored);
    expect(leftover).toHaveLength(1);
    expect(leftover[0]).toMatchObject({ oldLines: ["E"], newLines: ["Y"] });
  });

  test("T-U05 manual edit after undo becomes a new pending hunk", () => {
    const baseline = "A\nB\nC\nD\nE";
    const afterUndo = "A\nB_MANUAL\nC\nD\nY";
    const hunks = computeLineDiff(baseline, afterUndo);
    expect(hunks).toHaveLength(2);
    expect(hunks[0]).toMatchObject({ oldLines: ["B"], newLines: ["B_MANUAL"] });
    expect(hunks[1]).toMatchObject({ oldLines: ["E"], newLines: ["Y"] });
  });

  test("T-U06 reverse-patching a new file yields empty content", () => {
    const current = "export default {}";
    const [hunk] = computeLineDiff("", current);
    expect(applyReversePatch(current, hunk)).toBe("");
  });
});

describe("T-M 多轮多文件", () => {
  test("T-M01 keeps two files independent", () => {
    const aChunks = getActiveChunksForFile(
      "ws",
      "A.ts",
      "v1",
      [{ messageId: "t1", workspaceId: "ws", baselineContent: "v0" }],
      new Map(),
    );
    const bChunks = getActiveChunksForFile(
      "ws",
      "B.ts",
      "modified",
      [{ messageId: "t2", workspaceId: "ws", baselineContent: "original" }],
      new Map(),
    );
    expect(aChunks[0].hunk).toMatchObject({ oldLines: ["v0"], newLines: ["v1"] });
    expect(bChunks[0].hunk).toMatchObject({ oldLines: ["original"], newLines: ["modified"] });
    expect(aChunks[0].messageId).toBe("t1");
    expect(bChunks[0].messageId).toBe("t2");
  });

  test("T-M02 stacks later-turn inserts beside earlier pending hunks", () => {
    const current = "A\nX\nC\nY\nNEW_LINE";
    const chunks = getActiveChunksForFile(
      "ws",
      "A.ts",
      current,
      [
        { messageId: "t1", workspaceId: "ws", baselineContent: "A\nB\nC\nD" },
        { messageId: "t2", workspaceId: "ws", baselineContent: "A\nX\nC\nY" },
      ],
      new Map(),
    );
    expect(chunks.some((chunk) => chunk.messageId === "t1" && chunk.hunk.oldLines[0] === "B")).toBe(true);
    expect(chunks.some((chunk) => chunk.messageId === "t1" && chunk.hunk.oldLines.includes("D"))).toBe(true);
    expect(chunks.some((chunk) => chunk.messageId === "t2" && chunk.hunk.newLines.includes("NEW_LINE"))).toBe(true);
  });

  test("T-M03 accepted turn-2 chunk stays hidden", () => {
    const chunks = getActiveChunksForFile(
      "ws",
      "A.ts",
      "changed",
      [{ messageId: "t2", workspaceId: "ws", baselineContent: "original" }],
      new Map([[idOf("t2", "A.ts", "original", "changed"), "accepted"]]),
    );
    expect(chunks).toEqual([]);
  });
});

describe("T-E 边界", () => {
  test("T-E01 single-line replacement", () => {
    const [hunk] = computeLineDiff("only line", "replaced");
    expect(hunk).toMatchObject({ oldLines: ["only line"], newLines: ["replaced"] });
    expect(applyReversePatch("replaced", hunk)).toBe("only line");
  });

  test("T-E05 rename is delete old plus add new", () => {
    const removed = computeLineDiff("old content", "");
    const added = computeLineDiff("", "new content");
    expect(removed[0]).toMatchObject({ oldLines: ["old content"], newLines: [] });
    expect(added[0]).toMatchObject({ oldLines: [], newLines: ["new content"] });
  });

  test("T-E06 identical snapshot and disk yields no chunks", () => {
    expect(
      getActiveChunksForFile(
        "ws",
        "a.ts",
        "same",
        [{ messageId: "m1", workspaceId: "ws", baselineContent: "same" }],
        new Map(),
      ),
    ).toEqual([]);
  });

  test("T-E08 computes 50 discrete hunks within 100ms", () => {
    const baseline = Array.from({ length: 150 }, (_, index) => `keep-${index}`).join("\n");
    const current = Array.from({ length: 150 }, (_, index) =>
      index % 3 === 0 ? `edit-${index}` : `keep-${index}`,
    ).join("\n");
    const started = performance.now();
    const hunks = computeLineDiff(baseline, current);
    expect(performance.now() - started).toBeLessThan(100);
    expect(hunks.length).toBeGreaterThanOrEqual(50);
  });

  test("T-E10 keeps chunk identity by content hash after a header insert", () => {
    const baseline = [
      "l1",
      "l2",
      "l3",
      "l4",
      "OLD5",
      "OLD6",
      "OLD7",
      "OLD8",
      ...Array.from({ length: 11 }, (_, index) => `mid-${index}`),
      "OLD20",
      "OLD21",
      "OLD22",
    ].join("\n");
    const current = [
      "l1",
      "l2",
      "l3",
      "l4",
      "NEW5",
      "NEW6",
      "NEW7",
      "NEW8",
      ...Array.from({ length: 11 }, (_, index) => `mid-${index}`),
      "NEW20",
      "NEW21",
      "NEW22",
    ].join("\n");
    const original = computeLineDiff(baseline, current);
    expect(original).toHaveLength(2);
    expect(original[0].newStart).toBe(5);
    expect(original[1].newStart).toBe(20);

    const turns = [{ messageId: "t1", workspaceId: "ws", baselineContent: baseline }];
    const seeded = getActiveChunksForFile("ws", "a.ts", current, turns, new Map());
    expect(seeded).toHaveLength(2);
    expect(seeded[0].chunkKey).toBe(chunkKeyFor(original[0]));
    expect(seeded[1].chunkKey).toBe(chunkKeyFor(original[1]));

    const shifted = computeLineDiff(baseline, `// note\n${current}`);
    const first = shifted.find((hunk) => hunk.newLines[0] === "NEW5");
    const second = shifted.find((hunk) => hunk.newLines[0] === "NEW20");
    expect(first?.newStart).toBe(6);
    expect(second?.newStart).toBe(21);

    const states = new Map([
      [seeded[0].chunkId, "accepted" as const],
      [seeded[1].chunkId, "pending" as const],
    ]);
    const chunks = getActiveChunksForFile("ws", "a.ts", `// note\n${current}`, turns, states);
    expect(chunks.some((chunk) => chunk.hunk.newLines.includes("// note"))).toBe(true);
    expect(chunks.every((chunk) => chunk.chunkId !== seeded[0].chunkId)).toBe(true);
    const kept = chunks.find((chunk) => chunk.chunkKey === seeded[1].chunkKey);
    expect(kept?.hunk.newStart).toBe(21);
    expect(kept?.hunk.newLines[0]).toBe("NEW20");
    expect(kept?.status).toBe("pending");
  });
});
