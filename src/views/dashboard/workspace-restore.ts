import { normalizeBindPath } from "../../components/dashboard/workspace-picker";
import type { Workspace } from "../../domain/types";

const KEY = "multara.workspace.restore";

export type WorkspaceRestoreHint = {
  projectId: string;
  sourcePath: string;
  branch: string | null;
  upstream: string | null;
};

function readAll(): WorkspaceRestoreHint[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as WorkspaceRestoreHint[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function writeAll(hints: WorkspaceRestoreHint[]) {
  if (!hints.length) localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, JSON.stringify(hints));
}

export function rememberRestoreHints(hints: WorkspaceRestoreHint[]) {
  const incoming = new Set(
    hints.map((hint) => `${hint.projectId}\0${normalizeBindPath(hint.sourcePath)}`),
  );
  const kept = readAll().filter(
    (item) => !incoming.has(`${item.projectId}\0${normalizeBindPath(item.sourcePath)}`),
  );
  writeAll([...kept, ...hints]);
}

export function forgetRestoreHints(projectId: string) {
  writeAll(readAll().filter((item) => item.projectId !== projectId));
}

/** 这份需求里已经重新绑上的路径不再挡进入。 */
export function pendingRestoreHints(projectId: string, workspaces: Workspace[]) {
  const bound = new Set(
    workspaces
      .filter((item) => item.projectId === projectId)
      .map((item) => normalizeBindPath(item.sourcePath)),
  );
  return readAll().filter(
    (item) => item.projectId === projectId && !bound.has(normalizeBindPath(item.sourcePath)),
  );
}
