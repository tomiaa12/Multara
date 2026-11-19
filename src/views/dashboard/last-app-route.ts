import { readStorage, writeStorage } from "../../utils/safe-storage";

const STORAGE_KEY = "multara.lastAppRoute";

/** 不恢复的路径：根重定向、开发预览 */
function isRestorable(path: string) {
  return Boolean(path)
    && path !== "/"
    && path !== "/preview"
    && !path.startsWith("/preview/");
}

export function rememberAppRoute(path: string) {
  if (!isRestorable(path)) return;
  writeStorage(STORAGE_KEY, path);
}

export function lastAppRoute(fallback = "/projects") {
  const stored = readStorage(STORAGE_KEY);
  if (stored && isRestorable(stored)) return stored;
  return fallback;
}

export function clearLastAppRouteIf(match: (path: string) => boolean) {
  const stored = readStorage(STORAGE_KEY);
  if (stored && match(stored)) writeStorage(STORAGE_KEY, "/projects");
}
