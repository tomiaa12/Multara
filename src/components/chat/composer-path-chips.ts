import type { EditorSelectionContext } from "../../domain/types";
import * as backend from "../../services/backend";
import { fileName } from "./composer-content";
import { resolvePathInWorkspaces, type OsDroppedEntry } from "./workspace-file-drag";

/** 整文件 / 文件夹只建路径芯片，不读正文（对齐 VS Code Agent Host）。 */
export async function workspacePathChip(file: {
  workspaceId: string;
  path: string;
  isDirectory?: boolean;
  name?: string;
}): Promise<EditorSelectionContext> {
  let isDirectory = Boolean(file.isDirectory);
  if (file.isDirectory === undefined) {
    try {
      await backend.listWorkspaceDirectory(file.workspaceId, file.path);
      isDirectory = true;
    } catch {
      isDirectory = false;
    }
  }
  const base = fileName(file.path);
  return {
    id: crypto.randomUUID(),
    workspaceId: file.workspaceId,
    path: file.path,
    startLine: 0,
    endLine: 0,
    text: isDirectory && !base ? (file.name || "") : "",
    isDirectory,
  };
}

export async function externalPathChip(
  entry: OsDroppedEntry,
  workspaces: Array<{ id: string; workPath?: string | null; sourcePath?: string | null; name?: string | null }>,
): Promise<EditorSelectionContext | null> {
  const path = entry.path;
  if (!path && !entry.name) return null;
  const resolved = path ? resolvePathInWorkspaces(path, workspaces) : null;
  if (resolved) {
    const root = workspaces.find((item) => item.id === resolved.workspaceId);
    return workspacePathChip({
      ...resolved,
      isDirectory: resolved.path === "" || entry.isDirectory,
      name: entry.name || root?.name || fileName(path),
    });
  }
  if (!path) return null;
  let isDirectory = Boolean(entry.isDirectory);
  if (entry.isDirectory === undefined && !entry.file && entry.path) {
    try {
      await backend.listExternalDirectory(entry.path);
      isDirectory = true;
    } catch {
      isDirectory = false;
    }
  } else if (entry.file) {
    isDirectory = false;
  }
  const base = fileName(path);
  return {
    id: crypto.randomUUID(),
    workspaceId: "",
    path,
    startLine: 0,
    endLine: 0,
    text: isDirectory && !base ? (entry.name || "") : "",
    isDirectory,
  };
}
