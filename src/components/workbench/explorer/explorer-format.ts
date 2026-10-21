/** VS Code 资源管理器悬停提示里的体积与时间。 */

export function formatByteSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb < 10 ? kb.toFixed(1) : Math.round(kb)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  const gb = mb / 1024;
  return `${gb < 10 ? gb.toFixed(1) : Math.round(gb)} GB`;
}

export function formatEntryTime(mtime: number): string {
  if (!mtime) return "";
  const date = new Date(mtime);
  if (!Number.isFinite(date.getTime())) return "";
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function explorerHoverTitle(entry: {
  path: string;
  isDirectory: boolean;
  symlinkTarget?: string;
  size?: number;
  mtime?: number;
  compact?: { paths: string[] };
  excluded?: boolean;
}): string {
  const path = entry.compact?.paths.at(-1) ?? entry.path;
  const lines = [path];
  if (entry.excluded) lines.push("已从资源管理器排除，因文件已打开而显示");
  if (entry.symlinkTarget) lines.push(`符号链接 → ${entry.symlinkTarget}`);
  if (!entry.isDirectory && entry.size != null) lines.push(formatByteSize(entry.size));
  if (entry.mtime) lines.push(formatEntryTime(entry.mtime));
  return lines.filter(Boolean).join("\n");
}

export function highlightName(name: string, matches?: Array<[number, number]>): Array<{ text: string; hit: boolean }> {
  if (!matches?.length) return [{ text: name, hit: false }];
  const parts: Array<{ text: string; hit: boolean }> = [];
  let cursor = 0;
  for (const [start, end] of matches) {
    if (start > cursor) parts.push({ text: name.slice(cursor, start), hit: false });
    if (end > start) parts.push({ text: name.slice(start, end), hit: true });
    cursor = Math.max(cursor, end);
  }
  if (cursor < name.length) parts.push({ text: name.slice(cursor), hit: false });
  return parts.filter((part) => part.text);
}
