const STORAGE_KEY = "multara.lastProjectsPath";

export function rememberProjectsPath(path: string) {
  if (path !== "/projects" && !path.startsWith("/projects/")) return;
  sessionStorage.setItem(STORAGE_KEY, path);
}

export function lastProjectsPath() {
  return sessionStorage.getItem(STORAGE_KEY) || "/projects";
}

export function clearLastProjectsPath(projectId?: string) {
  const last = lastProjectsPath();
  if (!projectId || last === `/projects/${projectId}`) {
    sessionStorage.setItem(STORAGE_KEY, "/projects");
  }
}
