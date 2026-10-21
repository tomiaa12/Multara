import { describe, expect, test, vi } from "vitest";
import { formatRollbackDirtyMessage } from "../components/workbench/unsaved-editor";
import { countMessagesFrom, executeReask, reaskBannerText, reaskTouchesFiles, reshuffleHintText } from "./reask";

describe("executeReask", () => {
  test("T-E07 canceling dirty rollback aborts without writing disk", async () => {
    const rollbackToTurn = vi.fn();
    const truncateSession = vi.fn();
    const reloadDiff = vi.fn();
    const send = vi.fn();
    const result = await executeReask({
      rollback: true,
      dirtyPaths: ["src/a.ts"],
      confirmDirty: async (message) => {
        expect(message).toBe(formatRollbackDirtyMessage(["src/a.ts"]));
        return false;
      },
      rollbackToTurn,
      truncateSession,
      reloadDiff,
      send,
    });
    expect(result).toBe("aborted");
    expect(rollbackToTurn).not.toHaveBeenCalled();
    expect(truncateSession).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
  });

  test("T-E07 continue rollback restores snapshot then truncates", async () => {
    const rollbackToTurn = vi.fn();
    const truncateSession = vi.fn();
    const result = await executeReask({
      rollback: true,
      dirtyPaths: ["src/a.ts"],
      confirmDirty: async () => true,
      rollbackToTurn,
      truncateSession,
      reloadDiff: vi.fn(),
      send: vi.fn(),
    });
    expect(result).toBe("sent");
    expect(rollbackToTurn).toHaveBeenCalledOnce();
    expect(truncateSession).toHaveBeenCalledOnce();
  });

  test("T-R06 keep-code truncates without rollback", async () => {
    const rollbackToTurn = vi.fn();
    const disk = { a: "v1", b: "v2" };
    const result = await executeReask({
      rollback: false,
      dirtyPaths: ["src/a.ts"],
      confirmDirty: async () => false,
      rollbackToTurn,
      truncateSession: async () => undefined,
      reloadDiff: async () => undefined,
      send: () => undefined,
    });
    expect(result).toBe("sent");
    expect(rollbackToTurn).not.toHaveBeenCalled();
    expect(disk).toEqual({ a: "v1", b: "v2" });
  });

  test("skips the file prompt when this turn and later turns never snapshotted a file", () => {
    const messages = [{ id: "msg1" }, { id: "reply1" }, { id: "msg2" }, { id: "reply2" }];
    const files = new Map<string, { size: number }>([["msg1", { size: 1 }]]);
    expect(reaskTouchesFiles(messages, "msg2", files)).toBe(false);
    expect(reaskTouchesFiles(messages, "msg1", files)).toBe(true);
    expect(reaskTouchesFiles(messages, "msg2", new Map([["msg2", { size: 0 }]]))).toBe(false);
  });

  test("T-RA01 counts messages after the target, not the target itself", () => {
    const messages = [
      { id: "msg1" },
      { id: "reply1" },
      { id: "msg2" },
      { id: "reply2" },
    ];
    expect(countMessagesFrom(messages, "msg2")).toBe(1);
    expect(reshuffleHintText(1)).toBe("之后 1 条消息将被删除。");
    expect(reaskBannerText(1)).toContain("之后的 1 条消息将被清除");
  });

  test("T-RA06 cancel does not touch disk or send", async () => {
    const rollbackToTurn = vi.fn();
    const truncateSession = vi.fn();
    const send = vi.fn();
    const result = await executeReask({
      rollback: true,
      dirtyPaths: ["src/a.ts"],
      confirmDirty: async () => false,
      rollbackToTurn,
      truncateSession,
      reloadDiff: vi.fn(),
      send,
    });
    expect(result).toBe("aborted");
    expect(rollbackToTurn).not.toHaveBeenCalled();
    expect(truncateSession).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
  });

  test("rollback failure aborts when the user declines to delete the conversation", async () => {
    const truncateSession = vi.fn();
    const send = vi.fn();
    const result = await executeReask({
      rollback: true,
      dirtyPaths: [],
      confirmDirty: async () => true,
      confirmRollbackFailure: async (message) => {
        expect(message).toBe("disk locked");
        return false;
      },
      rollbackToTurn: async () => {
        throw new Error("disk locked");
      },
      truncateSession,
      reloadDiff: vi.fn(),
      send,
    });
    expect(result).toBe("aborted");
    expect(truncateSession).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
  });

  test("rollback failure can still truncate when the user confirms", async () => {
    const truncateSession = vi.fn();
    const result = await executeReask({
      rollback: true,
      dirtyPaths: [],
      confirmDirty: async () => true,
      confirmRollbackFailure: async () => true,
      rollbackToTurn: async () => {
        throw new Error("disk locked");
      },
      truncateSession,
      reloadDiff: vi.fn(),
      send: vi.fn(),
    });
    expect(result).toBe("sent");
    expect(truncateSession).toHaveBeenCalledOnce();
  });
});
