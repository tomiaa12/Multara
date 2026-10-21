import type { Task, TaskStatus } from "../domain/types";

export const TASK_COLUMNS: Array<{ id: TaskStatus; label: string; optional?: boolean }> = [
  { id: "backlog", label: "待规划" },
  { id: "todo", label: "待办" },
  { id: "in_progress", label: "进行中" },
  { id: "in_review", label: "审核中" },
  { id: "done", label: "已完成", optional: true },
  { id: "blocked", label: "已阻塞" },
  { id: "cancelled", label: "已取消", optional: true },
];

export function taskStatusLabel(status: TaskStatus) {
  return TASK_COLUMNS.find((column) => column.id === status)?.label ?? status;
}

export function taskCode(task: Task, tasks: Task[]) {
  const ordered = [...tasks].sort((a, b) => a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id));
  const index = ordered.findIndex((item) => item.id === task.id);
  return `TASK-${Math.max(index, 0) + 1}`;
}

export { formatRelativeTime as relativeTime } from "../components/ui/relative-time";

export function orderColumn(tasks: Task[], saved: string[]) {
  const rank = new Map(saved.map((id, index) => [id, index]));
  return [...tasks].sort((a, b) => {
    const left = rank.has(a.id) ? rank.get(a.id)! : -1;
    const right = rank.has(b.id) ? rank.get(b.id)! : -1;
    if (left !== right) return left - right;
    return b.updatedAt.localeCompare(a.updatedAt);
  });
}

export function moveTask(ids: string[], taskId: string, beforeId: string) {
  const next = ids.filter((id) => id !== taskId);
  if (!beforeId || beforeId === taskId) {
    next.push(taskId);
    return next;
  }
  const index = next.indexOf(beforeId);
  if (index < 0) next.push(taskId);
  else next.splice(index, 0, taskId);
  return next;
}

/**
 * 系统轻量同步：
 * - 开跑：backlog/todo → in_progress
 * - 成功结束且无活跃 run：in_progress → in_review（等人审；done 仍需显式写入）
 * - 失败且无活跃 run：in_progress → todo
 */
export function boardStatusFromRun(
  runStatus: string,
  current: TaskStatus,
  sessionStillActive: boolean,
): TaskStatus | null {
  if (current === "cancelled") return null;
  if (runStatus === "queued" || runStatus === "running") {
    if (current === "backlog" || current === "todo") return "in_progress";
    return null;
  }
  if (sessionStillActive) return null;
  if (runStatus === "completed" && current === "in_progress") return "in_review";
  if (runStatus === "failed" && current === "in_progress") return "todo";
  return null;
}
