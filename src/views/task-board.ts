import type { Task, TaskStatus } from "../domain/types";
import { t } from "../i18n";

const TASK_STATUS_KEY: Record<TaskStatus, string> = {
  backlog: "tasks.status.backlog",
  todo: "tasks.status.todo",
  in_progress: "tasks.status.in_progress",
  in_review: "tasks.status.in_review",
  done: "tasks.status.done",
  blocked: "tasks.status.blocked",
  cancelled: "tasks.status.cancelled",
};

const TASK_COLUMN_META: Array<{ id: TaskStatus; optional?: boolean }> = [
  { id: "backlog" },
  { id: "todo" },
  { id: "in_progress" },
  { id: "in_review" },
  { id: "done", optional: true },
  { id: "blocked" },
  { id: "cancelled", optional: true },
];

export function taskStatusLabel(status: TaskStatus) {
  return t(TASK_STATUS_KEY[status]);
}

export const TASK_COLUMNS: Array<{ id: TaskStatus; label: string; optional?: boolean }> = TASK_COLUMN_META.map((column) => ({
  ...column,
  get label() {
    return taskStatusLabel(column.id);
  },
}));

function byCreation(a: Task, b: Task) {
  return a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id);
}

export function taskCode(task: Task, tasks: Task[]) {
  const ordered = [...tasks].sort(byCreation);
  const index = ordered.findIndex((item) => item.id === task.id);
  return `TASK-${Math.max(index, 0) + 1}`;
}

/** 一次排序得出所有任务的编号，供列表按 id 查表。 */
export function taskCodeMap(tasks: Task[]) {
  return new Map([...tasks].sort(byCreation).map((task, index) => [task.id, `TASK-${index + 1}`]));
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
