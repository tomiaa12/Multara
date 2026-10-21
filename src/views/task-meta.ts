export type TaskPriority = "none" | "urgent" | "high" | "medium" | "low";

export interface TaskMeta {
  priority: TaskPriority;
  labels: string[];
}

export const PRIORITY_OPTIONS: Array<{ id: TaskPriority; label: string }> = [
  { id: "none", label: "无优先级" },
  { id: "urgent", label: "紧急" },
  { id: "high", label: "高" },
  { id: "medium", label: "中" },
  { id: "low", label: "低" },
];

const KEY = "multara.task-meta";

function emptyMeta(): TaskMeta {
  return { priority: "none", labels: [] };
}

function readAll(): Record<string, TaskMeta> {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, Partial<TaskMeta>>;
    if (!parsed || typeof parsed !== "object") return {};
    return Object.fromEntries(
      Object.entries(parsed).map(([id, item]) => [
        id,
        {
          priority: PRIORITY_OPTIONS.some((option) => option.id === item?.priority) ? item.priority! : "none",
          labels: Array.isArray(item?.labels) ? item.labels.filter((label) => typeof label === "string") : [],
        },
      ]),
    );
  } catch {
    return {};
  }
}

export function readTaskMeta(taskId: string): TaskMeta {
  return readAll()[taskId] ?? emptyMeta();
}

export function writeTaskMeta(taskId: string, patch: Partial<TaskMeta>) {
  const all = readAll();
  all[taskId] = { ...(all[taskId] ?? emptyMeta()), ...patch };
  localStorage.setItem(KEY, JSON.stringify(all));
}

export function priorityLabel(priority: TaskPriority) {
  return PRIORITY_OPTIONS.find((option) => option.id === priority)?.label ?? "无优先级";
}
