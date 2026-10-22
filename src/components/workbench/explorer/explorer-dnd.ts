import { t } from "../../../i18n";

export interface DragSource {
  workspaceId: string;
  path: string;
  isDirectory: boolean;
  readonly?: boolean;
}

export interface DropTarget {
  workspaceId: string;
  /** 目标文件夹路径；工作区根为空串。 */
  path: string;
  readonly?: boolean;
}

/** VS Code：Windows/Linux 按 Ctrl 复制，macOS 按 Option(Alt) 复制。 */
export function isCopyDrag(event: { ctrlKey: boolean; altKey: boolean }, isMac: boolean) {
  return isMac ? event.altKey : event.ctrlKey;
}

function parentOf(path: string) {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? "" : path.slice(0, slash);
}

function isEqualOrParent(path: string, candidateParent: string) {
  if (candidateParent === "") return true;
  return path === candidateParent || path.startsWith(`${candidateParent}/`);
}

/**
 * 移植 VS Code `FileDragAndDrop.handleDragOver` 里文件树内部拖放的判定：
 * 不能拖到自己上；不复制时不能拖回原父目录；不能把文件夹拖进它自己的子目录；
 * 只读目标不接受；全部只读的源只能复制。
 */
export function canDropInExplorer(sources: readonly DragSource[], target: DropTarget, isCopy: boolean): boolean {
  if (!sources.length) return false;
  if (target.readonly) return false;
  if (!isCopy && sources.every((source) => source.readonly)) return false;
  return !sources.some((source) => {
    if (source.workspaceId !== target.workspaceId) return false;
    if (source.path === target.path) return true;
    if (!isCopy && parentOf(source.path) === target.path) return true;
    if (source.path === "") return true;
    return isEqualOrParent(target.path, source.path);
  });
}

/** 落点为文件时改投到它所在的文件夹（VS Code 的 bubble up）。 */
export function dropFolderFor(entry: { path: string; isDirectory: boolean } | null): string {
  if (!entry) return "";
  return entry.isDirectory ? entry.path : parentOf(entry.path);
}

export function moveConfirmMessage(sources: readonly { path: string }[], targetName: string) {
  const name = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  if (sources.length > 1) {
    return {
      message: t("explorer.dnd.moveMany", { count: sources.length, target: targetName }),
      detail: fileNamesMessage(sources.map((source) => name(source.path))),
    };
  }
  return {
    message: t("explorer.dnd.moveOne", { name: name(sources[0].path), target: targetName }),
    detail: "",
  };
}

/** VS Code `getFileNamesMessage`：最多列 10 个，剩余的合并成一行。 */
export function fileNamesMessage(names: readonly string[]) {
  const max = 10;
  const lines = names.slice(0, max);
  if (names.length > max) {
    lines.push(
      names.length - max === 1
        ? t("explorer.dnd.moreFilesOne")
        : t("explorer.dnd.moreFilesMany", { count: names.length - max }),
    );
  }
  return lines.join("\n");
}
