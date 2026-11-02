import { reactive } from "vue";
import { readStorage, writeStorage } from "../utils/safe-storage";

export type ComposerRecentPlace = "workbench" | "float" | "chat" | "inbox" | "task";

export type ComposerRecentEntry = {
  path: string;
  /** 工作区内路径有值；系统绝对路径为空串 */
  workspaceId: string;
  isDirectory: boolean;
  label: string;
  at: number;
};

const MAX_RECENT = 20;
const STORAGE_PREFIX = "multara.composer.recent";

/** place +（仅 workbench）projectId → 最近列表 */
const buckets = reactive<Record<string, ComposerRecentEntry[]>>({});

function bucketKey(place: ComposerRecentPlace, projectId?: string | null) {
  if (place === "workbench") return `${place}:${projectId ?? ""}`;
  return place;
}

function storageKey(key: string) {
  return `${STORAGE_PREFIX}:${key}`;
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function readBucket(key: string): ComposerRecentEntry[] {
  try {
    const parsed = JSON.parse(readStorage(storageKey(key)) ?? "[]") as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.flatMap((item) => {
      if (!item || typeof item !== "object") return [];
      const row = item as Partial<ComposerRecentEntry>;
      if (typeof row.path !== "string" || !row.path) return [];
      return [{
        path: row.path,
        workspaceId: typeof row.workspaceId === "string" ? row.workspaceId : "",
        isDirectory: Boolean(row.isDirectory),
        label: typeof row.label === "string" && row.label ? row.label : fileName(row.path),
        at: typeof row.at === "number" ? row.at : 0,
      }];
    });
  } catch {
    return [];
  }
}

function ensureBucket(key: string) {
  if (!buckets[key]) buckets[key] = readBucket(key);
  return buckets[key];
}

function persist(key: string) {
  writeStorage(storageKey(key), JSON.stringify(buckets[key] ?? []));
}

export function listComposerRecent(
  place: ComposerRecentPlace,
  projectId?: string | null,
): ComposerRecentEntry[] {
  return ensureBucket(bucketKey(place, projectId));
}

export function recordComposerRecent(
  place: ComposerRecentPlace,
  entry: {
    path: string;
    workspaceId?: string;
    isDirectory?: boolean;
    label?: string;
  },
  projectId?: string | null,
) {
  const path = entry.path?.trim();
  if (!path) return;
  const key = bucketKey(place, projectId);
  const list = ensureBucket(key);
  const workspaceId = entry.workspaceId ?? "";
  const next: ComposerRecentEntry = {
    path,
    workspaceId,
    isDirectory: Boolean(entry.isDirectory),
    label: entry.label?.trim() || fileName(path),
    at: Date.now(),
  };
  buckets[key] = [
    next,
    ...list.filter((item) => !(item.path === path && item.workspaceId === workspaceId)),
  ].slice(0, MAX_RECENT);
  persist(key);
}
