export interface WorkspacePathRoot {
  id: string;
  name: string;
  roots: string[];
}

export interface LocatedFile {
  workspaceId: string;
  path: string;
}

export function normalizeFsPath(path: string) {
  return path.replaceAll("\\", "/").replace(/\/+$/, "");
}

export function isAbsoluteFsPath(path: string) {
  return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path);
}

export function relativeToolPath(raw: string) {
  return normalizeFsPath(raw).replace(/^\.\//, "");
}

export function pathWithoutWorktreeDir(path: string) {
  const normalized = relativeToolPath(path);
  return normalized.replace(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\//i, "");
}

export function isBareFileName(relative: string) {
  return Boolean(relative) && !relative.includes("/");
}

/** 裸文件名比文件名；带目录的引用允许命中更长路径上的后缀，例如 `views/index.vue` → `src/views/index.vue`。 */
export function filePathMatchesReference(filePath: string, reference: string) {
  const file = normalizeFsPath(filePath);
  const needle = relativeToolPath(reference);
  if (!file || !needle) return false;
  if (isBareFileName(needle)) return file.split("/").pop() === needle;
  return file === needle || file.endsWith(`/${needle}`);
}

export function matchLongestWorkspaceRoot(
  absolutePath: string,
  workspaces: WorkspacePathRoot[],
): LocatedFile | null {
  const path = normalizeFsPath(absolutePath);
  let best: { workspaceId: string; path: string; rootLength: number } | null = null;
  for (const workspace of workspaces) {
    for (const root of workspace.roots.map(normalizeFsPath)) {
      if (!root) continue;
      if (path !== root && !path.startsWith(`${root}/`)) continue;
      const relative = path === root ? "" : path.slice(root.length + 1);
      if (!relative) continue;
      if (!best || root.length > best.rootLength) {
        best = { workspaceId: workspace.id, path: relative, rootLength: root.length };
      }
    }
  }
  return best ? { workspaceId: best.workspaceId, path: best.path } : null;
}

export function workspaceFileLabel(name: string, path: string, multipleRoots: boolean) {
  if (!multipleRoots) return path;
  return path ? `${name} • ${path}` : name;
}
