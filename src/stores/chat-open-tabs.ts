const STORAGE_KEY = "multara.chat.openTabs";
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
  localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
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
  localStorage.removeItem(`${DISMISSED_PREFIX}:${tabScope(projectId)}`);
}
