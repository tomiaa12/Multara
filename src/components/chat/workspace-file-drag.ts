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
    if (parsed?.workspaceId && typeof parsed.path === "string" && parsed.path) {
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
          if (candidate === base) return null;
          if (candidate.startsWith(`${base}/`)) {
            return { workspaceId: workspace.id, path: candidate.slice(base.length + 1) };
          }
        }
      }
    }
  }
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
  const count = Math.max(items.length, files.length);
  for (let index = 0; index < count; index += 1) {
    const file = files[index] ?? items[index]?.getAsFile() ?? null;
    const fsEntry = items[index]?.webkitGetAsEntry?.() ?? null;
    const path = droppedFilePath(file);
    const name = entryName(path, file, fsEntry?.name ?? "");
    if (!path && !file && !name) continue;
    const key = path || `unnamed:${index}:${name}`;
    if (byKey.has(key)) continue;
    byKey.set(key, {
      path,
      name,
      isDirectory: fsEntry ? fsEntry.isDirectory : undefined,
      file: file ?? undefined,
    });
  }
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
  return preferDroppedFolders([...byKey.values()]);
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
