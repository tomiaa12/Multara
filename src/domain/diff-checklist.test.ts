import { describe, expect, test } from "vitest";
import { chunkKeyFor, makeChunkId } from "./diff";
import {
  applyReversePatch,
  computeLineDiff,
  getActiveChunksForFile,
} from "../utils/diff-engine";
import { formatRollbackDirtyMessage } from "../components/workbench/unsaved-editor";

describe("手动 checklist 自动化对照", () => {
  test("第一次 Edit 后立刻有 pending chunk，第二次扩大为累计 hunk", () => {
    const turns = [{ messageId: "m1", workspaceId: "ws", baselineContent: "A\nB\nC" }];
    const first = getActiveChunksForFile("ws", "a.ts", "A\nX\nC", turns, new Map());
    expect(first[0].hunk.newLines).toEqual(["X"]);
    const second = getActiveChunksForFile("ws", "a.ts", "A\nX\nY\nC", turns, new Map());
    expect(second).toHaveLength(1);
    expect(second[0].hunk.newLines).toEqual(["X", "Y"]);
  });

  test("Accept 后差异消失，磁盘内容仍是 AI 改后的版本", () => {
    const current = "A\nX\nC";
    const hidden = getActiveChunksForFile(
      "ws",
      "a.ts",
      current,
      [{ messageId: "m1", workspaceId: "ws", baselineContent: "A\nB\nC" }],
      new Map([[makeChunkId("m1", "ws", "a.ts", chunkKeyFor(computeLineDiff("A\nB\nC", current)[0])), "accepted"]]),
    );
    expect(hidden).toEqual([]);
    expect(current).toBe("A\nX\nC");
  });

  test("Undo 后文件回到 baseline", () => {
    const current = "A\nX\nC";
    const [hunk] = computeLineDiff("A\nB\nC", current);
    expect(applyReversePatch(current, hunk)).toBe("A\nB\nC");
  });

  test("绿色行末尾回车和中间插入都归入绿色", () => {
    expect(computeLineDiff("A\nB\nC", "A\nX\nY\n\nC")[0].newLines).toEqual(["X", "Y", ""]);
    expect(computeLineDiff("A\nB\nC", "A\nX\nMANUAL\nC")[0].newLines).toEqual(["X", "MANUAL"]);
  });

  test("两轮 diff 并排，Accept 第 2 轮后第 1 轮仍在", () => {
    const chunks = getActiveChunksForFile(
      "ws",
      "A.ts",
      "v1",
      [
        { messageId: "t1", workspaceId: "ws", baselineContent: "v0" },
        { messageId: "t2", workspaceId: "ws", baselineContent: "v1" },
      ],
      new Map([[makeChunkId("t2", "ws", "A.ts", chunkKeyFor({ oldLines: ["v1"], newLines: [] })), "accepted"]]),
    );
    expect(chunks).toHaveLength(1);
    expect(chunks[0].messageId).toBe("t1");
  });

  test("重新提问弹窗文案包含保持代码和回滚", () => {
    expect(formatRollbackDirtyMessage(["src/a.ts"])).toContain("Rollback 将覆盖：src/a.ts");
  });

  test("切换 session 不泄漏另一边的 pending chunk", () => {
    const sessionA = getActiveChunksForFile(
      "ws",
      "a.ts",
      "v1",
      [{ messageId: "a1", workspaceId: "ws", baselineContent: "v0" }],
      new Map(),
    );
    const sessionB = getActiveChunksForFile("ws", "a.ts", "v1", [], new Map());
    expect(sessionA).toHaveLength(1);
    expect(sessionB).toEqual([]);
  });
});
