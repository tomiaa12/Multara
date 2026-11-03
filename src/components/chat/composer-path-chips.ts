import type { EditorSelectionContext } from "../../domain/types";
import * as backend from "../../services/backend";
import { resolvePathInWorkspaces, type OsDroppedEntry } from "./workspace-file-drag";

/** 整文件 / 文件夹只建路径芯片，不读正文（对齐 VS Code Agent Host）。 */
export async function workspacePathChip(file: {
  workspaceId: string;
  path: string;
  isDirectory?: boolean;
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
  return {
    id: crypto.randomUUID(),
    workspaceId: file.workspaceId,
    path: file.path,
    startLine: 0,
    endLine: 0,
    text: "",
    isDirectory,
  };
}

export async function externalPathChip(
  entry: OsDroppedEntry,
  workspaces: Array<{ id: string; workPath?: string | null; sourcePath?: string | null }>,
  fallbackName: string,
): Promise<EditorSelectionContext> {
  const path = entry.path || entry.name || fallbackName;
  const resolved = entry.path ? resolvePathInWorkspaces(entry.path, workspaces) : null;
  if (resolved) {
    return workspacePathChip({ ...resolved, isDirectory: entry.isDirectory });
  }
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
  return {
    id: crypto.randomUUID(),
    workspaceId: "",
    path,
    startLine: 0,
    endLine: 0,
    text: "",
    isDirectory,
  };
}
