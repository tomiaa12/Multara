export const INBOX_TOOLS = ["changes", "files", "browser", "terminal"] as const;

export type InboxToolId = (typeof INBOX_TOOLS)[number];

export const INBOX_TOOL_STORAGE_KEY = "multara.inbox.tool";
export const INBOX_TOOL_TABS_KEY = "multara.inbox.toolTabs";
export const INBOX_TOOL_WIDTH_PREFIX = "multara.inbox.toolWidth.";
export const INBOX_PANEL_WIDTH_KEY = "multara.inbox.panelWidth";
export const INBOX_FILES_WIDTH_KEY = "multara.inbox.filesWidth";
export const INBOX_FILES_OPEN_KEY = "multara.inbox.filesOpen";
export const INBOX_TERMINAL_OPEN_KEY = "multara.inbox.terminalOpen";
export const INBOX_SURFACES_KEY = "multara.inbox.surfaces";
export const INBOX_TOOL_MIN_WIDTH = 280;
export const INBOX_TOOL_DEFAULT_WIDTH = 420;
export const INBOX_FILES_WIDTH = 280;
export const INBOX_RAIL_WIDTH = 44;

export const INBOX_SURFACES = ["changes", "browser", "search", "details", "canvas"] as const;
export type InboxSurfaceId = (typeof INBOX_SURFACES)[number];

export type InboxSurfaceTab = {
  key: string;
  kind: InboxSurfaceId;
};

let surfaceSeq = 0;

export function createInboxSurfaceTab(kind: InboxSurfaceId): InboxSurfaceTab {
  surfaceSeq += 1;
  return { key: `${kind}:${surfaceSeq}`, kind };
}

export function isInboxSurfaceId(value: string | null): value is InboxSurfaceId {
  return value === "changes" || value === "browser" || value === "search" || value === "details" || value === "canvas";
}

/** 更改/属性/Canvas 只保留一个；浏览器和搜索可重复。 */
function surfaceIsSingleton(kind: InboxSurfaceId) {
  return kind === "changes" || kind === "details" || kind === "canvas";
}

export function isInboxToolId(value: string | null): value is InboxToolId {
  return value === "changes" || value === "files" || value === "browser" || value === "terminal";
}

export function readInboxTool(): InboxToolId | "" {
  const stored = localStorage.getItem(INBOX_TOOL_STORAGE_KEY);
  if (isInboxToolId(stored)) return stored;
  return "";
}

export function readInboxToolTabs(): InboxToolId[] {
  try {
    const raw = JSON.parse(localStorage.getItem(INBOX_TOOL_TABS_KEY) || "[]");
    if (!Array.isArray(raw)) return [];
    const tabs: InboxToolId[] = [];
    for (const item of raw) {
      if (isInboxToolId(typeof item === "string" ? item : null) && !tabs.includes(item)) tabs.push(item);
    }
    return tabs;
  } catch {
    return [];
  }
}

export function readInboxToolWidth(id: InboxToolId): number {
  const stored = Number(localStorage.getItem(`${INBOX_TOOL_WIDTH_PREFIX}${id}`));
  if (stored >= INBOX_TOOL_MIN_WIDTH) return stored;
  return INBOX_TOOL_DEFAULT_WIDTH;
}

export function readInboxPanelWidth(): number {
  const stored = Number(localStorage.getItem(INBOX_PANEL_WIDTH_KEY));
  if (stored >= INBOX_TOOL_MIN_WIDTH) return stored;
  const previous = INBOX_TOOLS.map((id) => Number(localStorage.getItem(`${INBOX_TOOL_WIDTH_PREFIX}${id}`)))
    .find((width) => width >= INBOX_TOOL_MIN_WIDTH);
  return previous ?? INBOX_TOOL_DEFAULT_WIDTH;
}

export function readInboxFilesWidth(): number {
  const stored = Number(localStorage.getItem(INBOX_FILES_WIDTH_KEY));
  if (stored >= INBOX_TOOL_MIN_WIDTH) return stored;
  return INBOX_FILES_WIDTH;
}

function parseSurfaceKind(item: unknown): InboxSurfaceId | null {
  if (typeof item === "string" && isInboxSurfaceId(item)) return item;
  if (item && typeof item === "object" && "kind" in item) {
    const kind = (item as { kind?: unknown }).kind;
    if (typeof kind === "string" && isInboxSurfaceId(kind)) return kind;
  }
  return null;
}

export function readInboxSurfaces(): InboxSurfaceTab[] {
  try {
    const raw = JSON.parse(localStorage.getItem(INBOX_SURFACES_KEY) || "null");
    if (Array.isArray(raw)) {
      const tabs: InboxSurfaceTab[] = [];
      const seenSingleton = new Set<InboxSurfaceId>();
      for (const item of raw) {
        const kind = parseSurfaceKind(item);
        if (!kind) continue;
        if (surfaceIsSingleton(kind)) {
          if (seenSingleton.has(kind)) continue;
          seenSingleton.add(kind);
        }
        tabs.push(createInboxSurfaceTab(kind));
      }
      return tabs;
    }
  } catch {
    return [];
  }
  const migrated: InboxSurfaceTab[] = [];
  for (const id of readInboxToolTabs()) {
    if (id === "changes" || id === "browser") migrated.push(createInboxSurfaceTab(id));
  }
  return migrated;
}
