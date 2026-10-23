import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t } from "../i18n";

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function requireTauri(actionKey: string): void {
  if (!inTauri()) {
    throw new Error(t("explorer.browserPreviewBlocked", { action: t(actionKey) }));
  }
}

export interface ExplorerEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  depth: number;
  symlinkTarget?: string;
  mtime: number;
  size: number;
  readonly: boolean;
}

export interface ExplorerStat {
  size: number;
  mtime: number;
  ctime: number;
  readonly: boolean;
  isDirectory: boolean;
  isSymlink: boolean;
  absolutePath: string;
}

export interface ExplorerDeleteResult {
  path: string;
  backupId: string | null;
  error: string | null;
}

export interface ExplorerTransferRequest {
  /** 为空时 `source` 是本机绝对路径。 */
  sourceWorkspaceId: string | null;
  source: string;
  targetWorkspaceId: string;
  target: string;
}

export interface ExplorerTransferResult {
  target: string;
  error: string | null;
}

export interface ExplorerClipboardFile {
  path: string;
  name: string;
  isDirectory: boolean;
}

export interface ExplorerGitRemote {
  remote: string;
  branch: string;
  commit: string;
  prefix: string;
}

export interface ExplorerFsChange {
  workspaceId: string;
  paths: string[];
  git: boolean;
  overflow: boolean;
}

export const TRASH_FAILED_PREFIX = "TRASH_FAILED:";
export const TARGET_EXISTS_PREFIX = "EXISTS:";

export async function listDirectory(workspaceId: string, path?: string): Promise<ExplorerEntry[]> {
  if (!inTauri()) return [];
  return invoke<ExplorerEntry[]>("explorer_list_directory", { workspaceId, path: path || null });
}

export async function statEntry(workspaceId: string, path: string): Promise<ExplorerStat> {
  requireTauri("explorer.action.readStat");
  return invoke<ExplorerStat>("explorer_stat", { workspaceId, path });
}

export async function deleteEntries(
  workspaceId: string,
  paths: string[],
  useTrash: boolean,
): Promise<ExplorerDeleteResult[]> {
  requireTauri("explorer.action.deleteEntries");
  return invoke<ExplorerDeleteResult[]>("explorer_delete_entries", { workspaceId, paths, useTrash });
}

export async function restoreEntry(workspaceId: string, path: string, backupId: string): Promise<void> {
  requireTauri("explorer.action.restoreEntry");
  await invoke("explorer_restore_entry", { workspaceId, path, backupId });
}

export async function transferEntries(
  operations: ExplorerTransferRequest[],
  copy: boolean,
  overwrite = false,
): Promise<ExplorerTransferResult[]> {
  requireTauri(copy ? "explorer.action.copyEntries" : "explorer.action.moveEntries");
  return invoke<ExplorerTransferResult[]>("explorer_transfer_entries", { operations, copy, overwrite });
}

export async function createEntry(workspaceId: string, path: string, isDirectory: boolean): Promise<void> {
  requireTauri("explorer.action.createEntry");
  await invoke("explorer_create_entry", { workspaceId, path, isDirectory });
}

export async function writeFileBytes(
  workspaceId: string,
  path: string,
  bytes: Uint8Array,
  overwrite = false,
): Promise<void> {
  requireTauri("explorer.action.writeFile");
  await invoke("explorer_write_file", { workspaceId, path, base64: bytesToBase64(bytes), overwrite });
}

export async function gitIgnoredPaths(workspaceId: string, paths: string[]): Promise<string[]> {
  if (!inTauri() || paths.length === 0) return [];
  return invoke<string[]>("explorer_git_ignored", { workspaceId, paths });
}

export async function gitRemote(workspaceId: string): Promise<ExplorerGitRemote | null> {
  if (!inTauri()) return null;
  return invoke<ExplorerGitRemote | null>("explorer_git_remote", { workspaceId });
}

export async function openExternal(workspaceId: string, path: string): Promise<void> {
  requireTauri("explorer.action.openExternal");
  await invoke("explorer_open_external", { workspaceId, path });
}

export async function absolutePath(workspaceId: string, path: string): Promise<string> {
  requireTauri("explorer.action.readPath");
  return invoke<string>("explorer_absolute_path", { workspaceId, path });
}

export async function writeClipboardFiles(entries: Array<{ workspaceId: string; path: string }>): Promise<void> {
  if (!inTauri()) return;
  await invoke("explorer_clipboard_write_files", { entries });
}

export async function readClipboardFiles(): Promise<ExplorerClipboardFile[]> {
  if (!inTauri()) return [];
  return invoke<ExplorerClipboardFile[]>("explorer_clipboard_read_files");
}

export async function watchWorkspaces(owner: string, workspaceIds: string[], excludes: string[]): Promise<void> {
  if (!inTauri()) return;
  await invoke("explorer_watch_workspaces", { owner, workspaceIds, excludes });
}

export async function onFsChanged(handler: (change: ExplorerFsChange) => void): Promise<UnlistenFn> {
  if (!inTauri()) return () => undefined;
  return listen<ExplorerFsChange>("explorer-fs-changed", (event) => handler(event.payload));
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let index = 0; index < bytes.length; index += chunk) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunk));
  }
  return btoa(binary);
}
