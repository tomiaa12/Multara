import { listen } from "@tauri-apps/api/event";

const browserDev = import.meta.env.MULTARA_BROWSER_DEV === true;

function inTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 浏览器 DEV 时主窗口是隐藏的 invoke worker，界面事件要转给 Chrome。 */
export function browserDevWorker() {
  return browserDev && inTauri();
}

/** 桌面窗口把已经打开的终端告诉同一套 Vite 上的浏览器页面。 */
export function pushDevEvent(event: string, payload: unknown) {
  if (!import.meta.env.DEV || !inTauri()) return;
  void fetch("/__multara/push-events", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify([{ event, payload }]),
  }).catch(() => undefined);
}

type Handler = (event: { payload: unknown }) => void;

type EventSlot = {
  handlers: Map<string, Set<Handler>>;
  polling: boolean;
};

function eventSlot(): EventSlot {
  const host = window as Window & { __multaraDevEvents?: EventSlot };
  if (!host.__multaraDevEvents) {
    host.__multaraDevEvents = { handlers: new Map(), polling: false };
  }
  return host.__multaraDevEvents;
}

function deliver(event: string, payload: unknown) {
  eventSlot().handlers.get(event)?.forEach((handler) => {
    try {
      handler({ payload });
    } catch (error) {
      console.error("dev event", event, error);
    }
  });
}

async function poll() {
  const slot = eventSlot();
  while (slot.polling) {
    try {
      const response = await fetch("/__multara/events", { cache: "no-store" });
      if (response.ok) {
        const batch = (await response.json()) as Array<{ event?: string; payload?: unknown }>;
        for (const item of batch) {
          if (item?.event) deliver(item.event, item.payload);
        }
      }
    } catch {
      // 开发服务器还没起来
    }
    await new Promise((resolve) => window.setTimeout(resolve, 80));
  }
}

function ensurePoller() {
  if (inTauri() || !import.meta.env.DEV) return;
  const slot = eventSlot();
  if (slot.polling) return;
  slot.polling = true;
  void poll();
}

/** 桌面里直接听 Tauri。浏览器开发页听桌面窗口转过来的事件。隐藏的浏览器 DEV 窗口自己不处理。 */
export function onAppEvent<T>(
  name: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  if (browserDevWorker()) return Promise.resolve(() => undefined);
  if (inTauri()) {
    return listen<T>(name, handler).then((unlisten) => unlisten);
  }
  if (!import.meta.env.DEV) return Promise.resolve(() => undefined);
  ensurePoller();
  const wrapped: Handler = (event) => handler(event as { payload: T });
  const handlers = eventSlot().handlers;
  const set = handlers.get(name) ?? new Set();
  set.add(wrapped);
  handlers.set(name, set);
  return Promise.resolve(() => {
    set.delete(wrapped);
  });
}
