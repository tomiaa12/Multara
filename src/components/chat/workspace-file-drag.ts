export const WORKSPACE_FILE_MIME = "application/x-multara-workspace-file";
const WORKSPACE_FILE_TEXT_PREFIX = "multara-workspace:";
const DROP_ATTR = "data-workspace-drop";

export type WorkspaceFileDragPayload = {
  workspaceId: string;
  path: string;
  isDirectory?: boolean;
};

type DragListener = (state: { active: boolean; over: string | null }) => void;

let activeDrag: WorkspaceFileDragPayload | null = null;
let pointerSession: {
  payload: WorkspaceFileDragPayload;
  ghost: HTMLDivElement;
  started: boolean;
  startX: number;
  startY: number;
  pointerId: number;
  target: HTMLElement;
  onStart?: () => void;
  onMove?: (event: PointerEvent, zone: string | null) => void;
  onDrop?: (payload: WorkspaceFileDragPayload, zone: string | null, event: PointerEvent | null) => void;
  onCancel?: () => void;
} | null = null;
const listeners = new Set<DragListener>();

export function onWorkspaceFileDragChange(listener: DragListener) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function notify(over: string | null) {
  const active = Boolean(activeDrag);
  for (const listener of listeners) listener({ active, over });
}

function dropZoneFromPoint(x: number, y: number) {
  const element = document.elementFromPoint(x, y);
  return element?.closest(`[${DROP_ATTR}]`)?.getAttribute(DROP_ATTR) ?? null;
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function moveGhost(ghost: HTMLDivElement, x: number, y: number) {
  ghost.style.transform = `translate(${x + 12}px, ${y + 12}px)`;
}

function endPointerSession(zone: string | null, event: PointerEvent | null = null, cancelled = false) {
  const session = pointerSession;
  pointerSession = null;
  if (!session) {
    activeDrag = null;
    notify(null);
    return;
  }
  session.ghost.remove();
  document.body.classList.remove("is-dragging-workspace-file");
  session.target.removeEventListener("pointermove", onPointerMove);
  session.target.removeEventListener("pointerup", onPointerUp);
  session.target.removeEventListener("pointercancel", onPointerCancel);
  session.target.removeEventListener("lostpointercapture", onPointerCancel);
  window.removeEventListener("keydown", onDragKeydown, true);
  try {
    session.target.releasePointerCapture(session.pointerId);
  } catch {
    // Capture may already be released.
  }
  const payload = session.started ? session.payload : null;
  activeDrag = null;
  notify(null);
  if (!payload) return;
  if (cancelled) session.onCancel?.();
  else session.onDrop?.(payload, zone, event);
}

function onDragKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape" || !pointerSession?.started) return;
  event.preventDefault();
  event.stopPropagation();
  endPointerSession(null, null, true);
}

function onPointerMove(event: PointerEvent) {
  const session = pointerSession;
  if (!session || event.pointerId !== session.pointerId) return;
  const distance = Math.hypot(event.clientX - session.startX, event.clientY - session.startY);
  if (!session.started) {
    if (distance < 5) return;
    session.started = true;
    activeDrag = session.payload;
    document.body.classList.add("is-dragging-workspace-file");
    document.body.append(session.ghost);
    session.onStart?.();
  }
  event.preventDefault();
  moveGhost(session.ghost, event.clientX, event.clientY);
  const zone = dropZoneFromPoint(event.clientX, event.clientY);
  notify(zone);
  session.onMove?.(event, zone);
}

function onPointerUp(event: PointerEvent) {
  const session = pointerSession;
  if (!session || event.pointerId !== session.pointerId) return;
  const zone = session.started ? dropZoneFromPoint(event.clientX, event.clientY) : null;
  endPointerSession(zone, event);
}

function onPointerCancel(event: PointerEvent) {
  const session = pointerSession;
  if (!session || event.pointerId !== session.pointerId) return;
  endPointerSession(null, null, true);
}

export function beginWorkspaceFilePointerDrag(
  event: PointerEvent,
  payload: WorkspaceFileDragPayload,
  handlers: {
    onStart?: () => void;
    /** 拖动中每次移动都会调用，可读取修饰键与指针位置。 */
    onMove?: (event: PointerEvent, zone: string | null) => void;
    onDrop?: (payload: WorkspaceFileDragPayload, zone: string | null, event: PointerEvent | null) => void;
    /** Esc 或指针被系统取消。 */
    onCancel?: () => void;
    /** 拖影上的文字，默认是文件名；多选时可显示数量。 */
    label?: string;
  } = {},
) {
  if (event.button !== 0 || pointerSession) return;
  const target = event.currentTarget as HTMLElement | null;
  if (!target) return;
  const ghost = document.createElement("div");
  ghost.className = "workspace-file-drag-ghost";
  ghost.textContent = handlers.label ?? fileName(payload.path);
  ghost.style.pointerEvents = "none";
  pointerSession = {
    payload,
    ghost,
    started: false,
    startX: event.clientX,
    startY: event.clientY,
    pointerId: event.pointerId,
    target,
    onStart: handlers.onStart,
    onMove: handlers.onMove,
    onDrop: handlers.onDrop,
    onCancel: handlers.onCancel,
  };
  try {
    target.setPointerCapture(event.pointerId);
  } catch {
    pointerSession = null;
    return;
  }
  target.addEventListener("pointermove", onPointerMove);
  target.addEventListener("pointerup", onPointerUp);
  target.addEventListener("pointercancel", onPointerCancel);
  target.addEventListener("lostpointercapture", onPointerCancel);
  window.addEventListener("keydown", onDragKeydown, true);
}

export function setWorkspaceFileDragData(data: DataTransfer, payload: WorkspaceFileDragPayload) {
  activeDrag = payload;
  data.effectAllowed = "copy";
  data.setData("text/plain", `${WORKSPACE_FILE_TEXT_PREFIX}${JSON.stringify(payload)}`);
  try {
    data.setData(WORKSPACE_FILE_MIME, JSON.stringify(payload));
  } catch {
    // WebView2 may reject custom MIME types; text/plain + session payload is enough.
  }
}

export function clearWorkspaceFileDrag() {
  activeDrag = null;
  notify(null);
}

export function readWorkspaceFileDragData(data: DataTransfer | null): WorkspaceFileDragPayload | null {
  if (activeDrag) return activeDrag;
  const raw = data?.getData(WORKSPACE_FILE_MIME) || data?.getData("text/plain") || "";
  if (!raw) return null;
  const encoded = raw.startsWith(WORKSPACE_FILE_TEXT_PREFIX)
    ? raw.slice(WORKSPACE_FILE_TEXT_PREFIX.length)
    : raw;
  try {
    const parsed = JSON.parse(encoded) as WorkspaceFileDragPayload;
    // 项目根本身的 path 是空字符串，也要能拖进输入框。
    if (parsed?.workspaceId && typeof parsed.path === "string") {
      return {
        workspaceId: parsed.workspaceId,
        path: parsed.path,
        isDirectory: Boolean(parsed.isDirectory),
      };
    }
  } catch {
    return null;
  }
  return null;
}

export function isWorkspaceFileDrag(data: DataTransfer | null): boolean {
  if (activeDrag) return true;
  if (!data) return false;
  const types = Array.from(data.types);
  return types.includes(WORKSPACE_FILE_MIME) || types.includes("Files");
}

export function nativeFilePath(file: File): string {
  const path = (file as File & { path?: string }).path;
  return typeof path === "string" ? normalizeFsPath(path) : "";
}

/** macOS 拖出的路径常带 /private，工作区记录的是 /var 或 /tmp。 */
export function normalizeFsPath(path: string) {
  return path.replaceAll("\\", "/").replace(/\/+$/, "").replace(/^\/private\/(tmp|var)\//, "/$1/");
}

export function decodeDroppedFileUrl(value: string) {
  const line = value.trim();
  if (!line || line.startsWith("#")) return "";
  if (line.startsWith("file:")) {
    try {
      const url = new URL(line);
      let path = decodeURIComponent(url.pathname);
      if (/^\/[A-Za-z]:\//.test(path)) path = path.slice(1);
      return normalizeFsPath(path);
    } catch {
      return "";
    }
  }
  if (line.startsWith("/") || /^[A-Za-z]:[\\/]/.test(line)) return normalizeFsPath(line);
  return "";
}

function pathsFromUriList(data: DataTransfer) {
  const raw = data.getData("text/uri-list") || "";
  const plain = data.getData("text/plain") || "";
  const lines = (raw || (decodeDroppedFileUrl(plain) ? plain : "")).split(/\r?\n/);
  return lines.map(decodeDroppedFileUrl).filter(Boolean);
}

export function resolvePathInWorkspaces(
  raw: string,
  workspaces: Array<{ id: string; workPath?: string | null; sourcePath?: string | null }>,
): { workspaceId: string; path: string } | null {
  const path = normalizeFsPath(raw);
  if (!path) return null;
  const variants = [path, path.startsWith("/var/") || path.startsWith("/tmp/") ? `/private${path}` : ""].filter(Boolean);
  for (const workspace of workspaces) {
    for (const rootRaw of [workspace.workPath, workspace.sourcePath]) {
      const root = normalizeFsPath(rootRaw || "");
      if (!root) continue;
      const roots = [root, root.startsWith("/var/") || root.startsWith("/tmp/") ? `/private${root}` : ""].filter(Boolean);
      for (const candidate of variants) {
        for (const base of roots) {
          if (candidate === base) return { workspaceId: workspace.id, path: "" };
          if (candidate.startsWith(`${base}/`)) {
            return { workspaceId: workspace.id, path: candidate.slice(base.length + 1) };
          }
        }
      }
    }
  }
  return null;
}

export type ClipboardPasteAction =
  | { kind: "chip"; workspaceId: string; path: string; isDirectory: boolean }
  | { kind: "attachment"; path: string; name: string }
  | { kind: "image"; file: File }
  | { kind: "folder"; path: string; name: string };

/** 工作区内（含项目根）变成引用；外部文件变成附件；外部文件夹没有可发送的文件字节，仍作路径引用。 */
export function classifyClipboardEntry(
  entry: OsDroppedEntry,
  workspaces: Array<{ id: string; workPath?: string | null; sourcePath?: string | null }>,
): ClipboardPasteAction | null {
  const resolved = entry.path ? resolvePathInWorkspaces(entry.path, workspaces) : null;
  if (resolved) {
    return {
      kind: "chip",
      workspaceId: resolved.workspaceId,
      path: resolved.path,
      isDirectory: resolved.path === "" || Boolean(entry.isDirectory),
    };
  }
  if (entry.file?.type.startsWith("image/")) return { kind: "image", file: entry.file };
  const name = entry.name || entry.path.split(/[\\/]/).pop() || entry.path;
  if (entry.isDirectory && entry.path) return { kind: "folder", path: entry.path, name };
  if (entry.path) return { kind: "attachment", path: entry.path, name };
  return null;
}

export type OsDroppedEntry = {
  path: string;
  name: string;
  isDirectory?: boolean;
  file?: File;
};

function droppedFilePath(file: File | null) {
  return file ? nativeFilePath(file) : "";
}

function entryName(path: string, file: File | null, fallback = "") {
  return file?.name || fallback || path.split(/[\\/]/).pop() || path;
}

/** 从系统拖入的文件/文件夹；文件夹只保留自身，不展开成里面的文件。无本机路径时仍保留 File，方便读内容。 */
export function listOsDroppedEntries(data: DataTransfer | null): OsDroppedEntry[] {
  if (!data) return [];
  const files = [...data.files];
  const items = [...data.items];
  const byKey = new Map<string, OsDroppedEntry>();
  const seenFiles = new Set<string>();
  files.forEach((file, index) => {
    const fsEntry = items[index]?.webkitGetAsEntry?.() ?? null;
    pushDroppedEntry(byKey, seenFiles, index, file, fsEntry?.name ?? "", fsEntry ? fsEntry.isDirectory : undefined);
  });
  items.forEach((item, index) => {
    if (files[index]) return;
    const file = item.getAsFile?.() ?? null;
    const fsEntry = item.webkitGetAsEntry?.() ?? null;
    if (!file && !fsEntry) return;
    pushDroppedEntry(byKey, seenFiles, index, file, fsEntry?.name ?? "", fsEntry ? fsEntry.isDirectory : undefined);
  });
  // Tauri / WKWebView 上 File.path 经常是空的，Finder 会带 file:// URI。
  for (const path of pathsFromUriList(data)) {
    if ([...byKey.values()].some((item) => item.path === path)) continue;
    const name = path.split(/[\\/]/).pop() || path;
    const nameless = [...byKey.entries()].find(([, item]) => !item.path && item.name === name);
    if (nameless) {
      byKey.delete(nameless[0]);
      byKey.set(path, { ...nameless[1], path, name });
      continue;
    }
    byKey.set(path, { path, name });
  }
  return preferDroppedFolders(markContainingDirectories(dedupeClipboardImages([...byKey.values()])));
}

function pushDroppedEntry(
  byKey: Map<string, OsDroppedEntry>,
  seenFiles: Set<string>,
  index: number,
  file: File | null,
  fallbackName: string,
  isDirectory: boolean | undefined,
) {
  const path = droppedFilePath(file);
  const name = entryName(path, file, fallbackName);
  if (!path && !file && !name) return;
  const fingerprint = file ? `${file.type}:${file.size}:${file.name}` : "";
  if (fingerprint && seenFiles.has(fingerprint)) return;
  if (fingerprint) seenFiles.add(fingerprint);
  const key = path || `unnamed:${index}:${name}`;
  if (byKey.has(key)) return;
  byKey.set(key, {
    path,
    name,
    isDirectory,
    file: file ?? undefined,
  });
}

/** 截图粘贴常同时给出 PNG 和 TIFF。没有独立路径时只留一张，优先非 TIFF。 */
export function dedupeClipboardImages(entries: OsDroppedEntry[]): OsDroppedEntry[] {
  const images = entries.filter((entry) => entry.file?.type.startsWith("image/") && !entry.path);
  if (images.length < 2) return entries;
  const keepTiff = !images.some((entry) => entry.file && entry.file.type !== "image/tiff");
  const seen = new Set<string>();
  const drop = new Set<OsDroppedEntry>();
  for (const entry of images) {
    const file = entry.file;
    if (!file) continue;
    if (!keepTiff && file.type === "image/tiff") {
      drop.add(entry);
      continue;
    }
    const key = `${file.type}:${file.size}:${file.name}`;
    if (seen.has(key)) drop.add(entry);
    else seen.add(key);
  }
  if (!drop.size) return entries;
  return entries.filter((entry) => !drop.has(entry));
}

/**
 * 系统拖放在各环境里形状不同：macOS 常同时给出文件夹和其中的文件，
 * Windows 用反斜杠，WKWebView 经常不标 isDirectory。
 * 只要一个路径是另一个路径的父级，它就是文件夹，子文件不再单独放入。
 */
export function markContainingDirectories(entries: OsDroppedEntry[]): OsDroppedEntry[] {
  const paths = entries.map((entry) => normalizeFsPath(entry.path)).filter(Boolean);
  return entries.map((entry) => {
    const path = normalizeFsPath(entry.path);
    const containsAnother = Boolean(path) && paths.some((other) => other !== path && other.startsWith(`${path}/`));
    if (!entry.isDirectory && !containsAnother) return entry;
    return { ...entry, path: path || entry.path, isDirectory: true };
  });
}

export function osPathEntries(paths: readonly string[]): OsDroppedEntry[] {
  const entries = paths.filter(Boolean).map((raw) => {
    const path = normalizeFsPath(raw);
    return { path, name: path.split("/").pop() || path };
  });
  return preferDroppedFolders(markContainingDirectories(entries));
}

export function preferDroppedFolders(entries: OsDroppedEntry[]): OsDroppedEntry[] {
  const dirs = entries.filter((entry) => entry.isDirectory);
  if (!dirs.length) return entries;
  return entries.filter((entry) => {
    if (entry.isDirectory) return true;
    const path = (entry.path || "").replaceAll("\\", "/");
    if (!path) return true;
    return !dirs.some((dir) => {
      const root = (dir.path || "").replaceAll("\\", "/");
      if (!root) return false;
      return path === root || path.startsWith(`${root}/`);
    });
  });
}
