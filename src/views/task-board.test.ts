import { beforeAll, describe, expect, it } from "vitest";
import { applyLanguageSetting } from "../i18n";
import type { Task } from "../domain/types";
import { moveTask, orderColumn, relativeTime, taskCode, boardStatusFromRun } from "./task-board";

beforeAll(() => applyLanguageSetting("zh-CN"));

function task(id: string, updatedAt: string, createdAt = updatedAt): Task {
  return {
    id,
    sessionId: id,
    title: id,
    description: "",
    agentId: "miku",
    status: "todo",
    createdAt,
    updatedAt,
  };
}

describe("task board order", () => {
  it("puts unsaved tasks ahead of a saved order", () => {
    const tasks = [task("a", "2026-01-02"), task("b", "2026-01-03"), task("c", "2026-01-01")];
    expect(orderColumn(tasks, ["c", "a"]).map((item) => item.id)).toEqual(["b", "c", "a"]);
  });

  it("inserts a card before another card and appends when the target is empty", () => {
    expect(moveTask(["a", "b", "c"], "a", "c")).toEqual(["b", "a", "c"]);
    expect(moveTask(["a", "b"], "a", "")).toEqual(["b", "a"]);
  });

  it("numbers tasks by creation time and formats relative time", () => {
    const tasks = [task("late", "2026-02-01", "2026-02-01"), task("early", "2026-01-01", "2026-01-01")];
    expect(taskCode(tasks[1], tasks)).toBe("TASK-1");
    expect(relativeTime("2026-01-01T00:00:00.000Z", Date.parse("2026-01-01T00:03:00.000Z"))).toBe("3 分钟前");
  });

  it("maps run status onto board columns lightly", () => {
    expect(boardStatusFromRun("running", "todo", false)).toBe("in_progress");
    expect(boardStatusFromRun("queued", "backlog", false)).toBe("in_progress");
    expect(boardStatusFromRun("queued", "done", false)).toBeNull();
    expect(boardStatusFromRun("completed", "in_progress", false)).toBe("in_review");
    expect(boardStatusFromRun("completed", "in_progress", true)).toBeNull();
    expect(boardStatusFromRun("completed", "done", false)).toBeNull();
    expect(boardStatusFromRun("failed", "in_progress", false)).toBe("todo");
    expect(boardStatusFromRun("failed", "in_progress", true)).toBeNull();
    expect(boardStatusFromRun("failed", "in_review", false)).toBeNull();
    expect(boardStatusFromRun("stopped", "in_progress", false)).toBeNull();
    expect(boardStatusFromRun("running", "cancelled", false)).toBeNull();
  });
});
