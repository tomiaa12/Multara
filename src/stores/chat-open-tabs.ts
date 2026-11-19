import { removeStorage, writeStorage } from "../utils/safe-storage";

const STORAGE_KEY = "multara.chat.openTabs";
const ACTIVE_STORAGE_KEY = "multara.chat.activeTabs";
const DISMISSED_PREFIX = "multara.chat.dismissedTabs";

export function tabScope(projectId: string | null | undefined) {
  return projectId ?? "global";
}

export function loadOpenTabsMap(): Record<string, string[]> {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter((entry): entry is [string, string[]] =>
        Array.isArray(entry[1]) && entry[1].every((id) => typeof id === "string"),
      ),
    );
  } catch {
    return {};
  }
}

export function persistOpenTabsMap(map: Record<string, string[]>) {
  writeStorage(STORAGE_KEY, JSON.stringify(map));
}

/** 每个需求 / 全局作用域上次选中的聊天 tab。 */
export function loadActiveTabsMap(): Record<string, string> {
  try {
    const parsed = JSON.parse(localStorage.getItem(ACTIVE_STORAGE_KEY) ?? "{}") as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(
        (entry): entry is [string, string] => typeof entry[0] === "string" && typeof entry[1] === "string" && entry[1].length > 0,
      ),
    );
  } catch {
    return {};
  }
}

export function persistActiveTabsMap(map: Record<string, string>) {
  writeStorage(ACTIVE_STORAGE_KEY, JSON.stringify(map));
}

export function loadLegacyDismissed(projectId: string | null | undefined) {
  try {
    const parsed = JSON.parse(localStorage.getItem(`${DISMISSED_PREFIX}:${tabScope(projectId)}`) ?? "[]");
    return new Set(Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === "string") : []);
  } catch {
    return new Set<string>();
  }
}

export function clearLegacyDismissed(projectId: string | null | undefined) {
  removeStorage(`${DISMISSED_PREFIX}:${tabScope(projectId)}`);
}
