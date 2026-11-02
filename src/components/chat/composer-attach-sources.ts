import type { ComputedRef, InjectionKey } from "vue";

/** 工作台已打开编辑器，供输入框「文件/文件夹」二级菜单使用 */
export type ComposerAttachOpenFile = {
  workspaceId: string;
  path: string;
  isDirectory?: boolean;
};

export const composerOpenEditorsKey: InjectionKey<ComputedRef<ComposerAttachOpenFile[]>> =
  Symbol("composerOpenEditors");

export const SYSTEM_PICK_ID = "pick:system";

export function fileSuggestId(workspaceId: string, path: string) {
  return `file:${encodeURIComponent(workspaceId)}:${encodeURIComponent(path)}`;
}

export function parseFileSuggestId(id: string): { workspaceId: string; path: string } | null {
  if (!id.startsWith("file:")) return null;
  const rest = id.slice("file:".length);
  const sep = rest.indexOf(":");
  if (sep < 0) return null;
  try {
    return {
      workspaceId: decodeURIComponent(rest.slice(0, sep)),
      path: decodeURIComponent(rest.slice(sep + 1)),
    };
  } catch {
    return null;
  }
}

export function pathFileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}
