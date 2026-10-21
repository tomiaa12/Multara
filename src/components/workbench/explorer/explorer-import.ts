import { nativeFilePath } from "../../chat/workspace-file-drag";

/** 从系统拖入的一项：有本机路径时直接让后端复制；没有时读出内容再写入。 */
export interface DroppedItem {
  name: string;
  isDirectory: boolean;
  nativePath: string;
  file?: File;
  entry?: FileSystemEntry;
}

export interface DroppedFileData {
  /** 相对拖入项自身的路径，例如 `docs/a.md`。 */
  relativePath: string;
  isDirectory: boolean;
  file?: File;
}

/** 必须在 drop 事件里同步调用：事件结束后 DataTransferItem 就失效了。 */
export function collectDroppedItems(data: DataTransfer | null): DroppedItem[] {
  if (!data) return [];
  const items = [...data.items].filter((item) => item.kind === "file");
  const files = [...data.files];
  const out: DroppedItem[] = [];
  const count = Math.max(items.length, files.length);
  for (let index = 0; index < count; index += 1) {
    const item = items[index];
    const entry = item?.webkitGetAsEntry?.() ?? undefined;
    const file = files[index] ?? item?.getAsFile() ?? undefined;
    const name = entry?.name || file?.name || "";
    if (!name) continue;
    out.push({
      name,
      isDirectory: entry ? entry.isDirectory : false,
      nativePath: file ? nativeFilePath(file) : "",
      file,
      entry,
    });
  }
  return out;
}

function readDirectoryEntries(reader: FileSystemDirectoryReader): Promise<FileSystemEntry[]> {
  return new Promise((resolve, reject) => reader.readEntries(resolve, reject));
}

function entryFile(entry: FileSystemFileEntry): Promise<File> {
  return new Promise((resolve, reject) => entry.file(resolve, reject));
}

/** 递归读取拖入的文件夹；`limit` 防止误拖整个磁盘把内存撑爆。 */
export async function readDroppedTree(item: DroppedItem, limit = 5000): Promise<DroppedFileData[]> {
  if (!item.isDirectory || !item.entry) {
    return item.file ? [{ relativePath: item.name, isDirectory: false, file: item.file }] : [];
  }
  const out: DroppedFileData[] = [{ relativePath: item.name, isDirectory: true }];
  const queue: Array<{ entry: FileSystemDirectoryEntry; path: string }> = [
    { entry: item.entry as FileSystemDirectoryEntry, path: item.name },
  ];
  while (queue.length) {
    const { entry, path } = queue.shift()!;
    const reader = entry.createReader();
    for (;;) {
      const batch = await readDirectoryEntries(reader);
      if (!batch.length) break;
      for (const child of batch) {
        if (out.length >= limit) throw new Error(`拖入的文件过多（超过 ${limit} 个），请改用复制粘贴`);
        const childPath = `${path}/${child.name}`;
        if (child.isDirectory) {
          out.push({ relativePath: childPath, isDirectory: true });
          queue.push({ entry: child as FileSystemDirectoryEntry, path: childPath });
        } else {
          out.push({ relativePath: childPath, isDirectory: false, file: await entryFile(child as FileSystemFileEntry) });
        }
      }
    }
  }
  return out;
}
