import { describe, expect, test } from "vitest";
import { makeChunkId, parseChunkId } from "./diff";

describe("chunkId", () => {
  test("round-trips a path that contains colons", () => {
    const chunkId = makeChunkId("msg", "ws", "C:/repo/src/a.ts", "abc123def456");
    expect(parseChunkId(chunkId)).toEqual({
      messageId: "msg",
      workspaceId: "ws",
      filePath: "C:/repo/src/a.ts",
      chunkKey: "abc123def456",
    });
  });
});
