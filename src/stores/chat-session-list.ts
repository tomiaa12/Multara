import { SESSION_LIST_DEFAULT_WIDTH } from "../components/chat/session-list";
import { readStorage, writeStorage } from "../utils/safe-storage";

const STORAGE_KEY = "multara.chat.sessionList";

/** 并排模式下列表是否展开、宽多少；按聊天场景（气泡 / 工作台）分开记 */
export interface SessionListPrefs {
  open: boolean;
  width: number;
}

const DEFAULT_PREFS: SessionListPrefs = { open: true, width: SESSION_LIST_DEFAULT_WIDTH };

function loadAll(): Record<string, SessionListPrefs> {
  try {
    const parsed = JSON.parse(readStorage(STORAGE_KEY) ?? "{}") as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    return parsed as Record<string, SessionListPrefs>;
  } catch {
    return {};
  }
}

export function loadSessionListPrefs(place: string): SessionListPrefs {
  const saved = loadAll()[place];
  return {
    open: typeof saved?.open === "boolean" ? saved.open : DEFAULT_PREFS.open,
    width: typeof saved?.width === "number" && Number.isFinite(saved.width) ? saved.width : DEFAULT_PREFS.width,
  };
}

export function saveSessionListPrefs(place: string, prefs: SessionListPrefs) {
  writeStorage(STORAGE_KEY, JSON.stringify({ ...loadAll(), [place]: prefs }));
}
