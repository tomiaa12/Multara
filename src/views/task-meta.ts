import { t } from "../i18n";
import { readStorage, writeStorage } from "../utils/safe-storage";

export type TaskPriority = "none" | "urgent" | "high" | "medium" | "low";

export interface TaskMeta {
  priority: TaskPriority;
  labels: string[];
}

const PRIORITY_KEYS: Record<TaskPriority, string> = {
  none: "tasks.priority.none",
  urgent: "tasks.priority.urgent",
  high: "tasks.priority.high",
  medium: "tasks.priority.medium",
  low: "tasks.priority.low",
};

export const PRIORITY_OPTIONS: Array<{ id: TaskPriority }> = [
  { id: "none" },
  { id: "urgent" },
  { id: "high" },
  { id: "medium" },
  { id: "low" },
];

const KEY = "multara.task-meta";

function emptyMeta(): TaskMeta {
  return { priority: "none", labels: [] };
}

let cache: Record<string, TaskMeta> | null = null;

/** 只解析一次，之后由 writeTaskMeta 同步更新；列表筛选时不必每个任务都整库 JSON.parse。 */
function readAll(): Record<string, TaskMeta> {
  cache ??= parseAll(readStorage(KEY));
  return cache;
}

/** 测试用：丢弃缓存，下次从 storage 重新解析。 */
export function resetTaskMetaCache() {
  cache = null;
}

function parseAll(raw: string | null): Record<string, TaskMeta> {
  try {
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
  const all = { ...readAll() };
  all[taskId] = { ...(all[taskId] ?? emptyMeta()), ...patch };
  cache = all;
  writeStorage(KEY, JSON.stringify(all));
}

export function priorityLabel(priority: TaskPriority) {
  return t(PRIORITY_KEYS[priority] ?? PRIORITY_KEYS.none);
}
