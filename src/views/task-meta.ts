import { readStorage, writeStorage } from "../utils/safe-storage";

export interface TaskMeta {
  labels: string[];
}

const KEY = "multara.task-meta";

function emptyMeta(): TaskMeta {
  return { labels: [] };
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
