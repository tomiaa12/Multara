export interface LocalHistoryEntry {
  id: string;
  workspaceId: string;
  path: string;
  savedAt: number;
  source: string;
  content: string;
}

export interface TimelineEntry {
  id: string;
  source: "local" | "git";
  label: string;
  detail: string;
  at: number;
}

const STORAGE_KEY = "multara.localHistory";
const MAX_PER_FILE = 20;
const MAX_ENTRIES = 200;
const MAX_CONTENT = 200_000;

const extras: LocalHistoryEntry[] = [];
const listeners = new Set<() => void>();

function readStored(): LocalHistoryEntry[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) || "[]");
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function writeStored(entries: LocalHistoryEntry[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
}

function notify() {
  for (const listener of listeners) listener();
}

export function onLocalHistoryChange(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function registerLocalHistory(entries: LocalHistoryEntry[]) {
  for (const entry of entries) {
    const index = extras.findIndex((item) => item.id === entry.id);
    if (index >= 0) extras[index] = entry;
    else extras.push(entry);
  }
}

export function listLocalHistory(workspaceId: string, path: string) {
  return [...readStored(), ...extras]
    .filter((item) => item.workspaceId === workspaceId && item.path === path)
    .sort((a, b) => b.savedAt - a.savedAt);
}

export function localHistoryEntry(id: string) {
  return readStored().find((item) => item.id === id) ?? extras.find((item) => item.id === id);
}

export function recordLocalHistory(input: {
  workspaceId: string;
  path: string;
  source: string;
  content: string;
  savedAt?: number;
}) {
  if (!input.path || input.content.length > MAX_CONTENT) return;
  const all = readStored();
  const mine = all
    .filter((item) => item.workspaceId === input.workspaceId && item.path === input.path)
    .sort((a, b) => b.savedAt - a.savedAt);
  if (mine[0]?.content === input.content) return;
  const next: LocalHistoryEntry = {
    id: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    workspaceId: input.workspaceId,
    path: input.path,
    source: input.source,
    content: input.content,
    savedAt: input.savedAt ?? Date.now(),
  };
  const others = all.filter(
    (item) => item.workspaceId !== input.workspaceId || item.path !== input.path,
  );
  writeStored([next, ...mine].slice(0, MAX_PER_FILE).concat(others).slice(0, MAX_ENTRIES));
  notify();
}

export function relativeTimelineTime(at: number, now = Date.now()) {
  const delta = Math.max(0, now - at);
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (delta < minute) return "刚刚";
  if (delta < hour) return `${Math.floor(delta / minute)} 分钟前`;
  if (delta < day) return `${Math.floor(delta / hour)} 小时前`;
  if (delta < 2 * day) return "昨天";
  if (delta < 7 * day) return `${Math.floor(delta / day)} 天前`;
  const date = new Date(at);
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
}

export function mergeTimeline(entries: TimelineEntry[], now = Date.now()) {
  const sorted = [...entries].sort((a, b) => b.at - a.at || a.label.localeCompare(b.label));
  return sorted.map((entry, index) => {
    const timestamp = entry.at > 0 ? relativeTimelineTime(entry.at, now) : "";
    const previous = index > 0 && sorted[index - 1].at > 0
      ? relativeTimelineTime(sorted[index - 1].at, now)
      : "";
    return {
      ...entry,
      timestamp,
      hideTimestamp: Boolean(timestamp) && timestamp === previous,
    };
  });
}

export function timelineTime(value: string) {
  const at = Date.parse(value);
  return Number.isFinite(at) ? at : 0;
}

export function absoluteTimelineTime(at: number) {
  const date = new Date(at);
  if (!Number.isFinite(date.getTime())) return "";
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** 文件重命名后把本地历史跟过去，对应 VS Code Timeline 对 rename 的跟踪。 */
export function renameLocalHistoryPath(workspaceId: string, from: string, to: string) {
  const all = readStored();
  let changed = false;
  const next = all.map((item) => {
    if (item.workspaceId !== workspaceId) return item;
    if (item.path === from) {
      changed = true;
      return { ...item, path: to };
    }
    if (item.path.startsWith(`${from}/`)) {
      changed = true;
      return { ...item, path: `${to}${item.path.slice(from.length)}` };
    }
    return item;
  });
  for (const item of extras) {
    if (item.workspaceId !== workspaceId) continue;
    if (item.path === from) item.path = to;
    else if (item.path.startsWith(`${from}/`)) item.path = `${to}${item.path.slice(from.length)}`;
  }
  if (changed) writeStored(next);
  if (changed || extras.some((item) => item.workspaceId === workspaceId)) notify();
}

export function deleteLocalHistory(id: string) {
  const stored = readStored();
  const next = stored.filter((item) => item.id !== id);
  const extraIndex = extras.findIndex((item) => item.id === id);
  if (next.length === stored.length && extraIndex < 0) return;
  if (next.length !== stored.length) writeStored(next);
  if (extraIndex >= 0) extras.splice(extraIndex, 1);
  notify();
}

export function renameLocalHistorySource(id: string, source: string) {
  const trimmed = source.trim();
  if (!trimmed) return;
  const stored = readStored();
  const index = stored.findIndex((item) => item.id === id);
  if (index >= 0) {
    stored[index] = { ...stored[index], source: trimmed };
    writeStored(stored);
    notify();
    return;
  }
  const extra = extras.find((item) => item.id === id);
  if (extra) extra.source = trimmed;
  notify();
}
