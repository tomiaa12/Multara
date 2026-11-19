import { readStorage, writeStorage } from "../../utils/safe-storage";
import { clearLastAppRouteIf } from "./last-app-route";

const STORAGE_KEY = "multara.lastProjectsPath";

function readLegacySession(): string | null {
  try {
    return sessionStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

export function rememberProjectsPath(path: string) {
  if (path !== "/projects" && !path.startsWith("/projects/")) return;
  writeStorage(STORAGE_KEY, path);
}

export function lastProjectsPath() {
  const stored = readStorage(STORAGE_KEY) || readLegacySession();
  if (stored && (stored === "/projects" || stored.startsWith("/projects/"))) return stored;
  return "/projects";
}

export function clearLastProjectsPath(projectId?: string) {
  const last = lastProjectsPath();
  if (!projectId || last === `/projects/${projectId}` || last.startsWith(`/projects/${projectId}/`)) {
    writeStorage(STORAGE_KEY, "/projects");
  }
  clearLastAppRouteIf((path) =>
    !projectId
      ? path === "/projects" || path.startsWith("/projects/")
      : path === `/projects/${projectId}` || path.startsWith(`/projects/${projectId}/`),
  );
}
