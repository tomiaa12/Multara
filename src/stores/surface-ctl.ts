import { listen } from "@tauri-apps/api/event";
import * as backend from "../services/backend";

export type SurfaceCtlPayload = {
  requestId: string;
  sessionId?: string;
  method: string;
  path?: string | null;
  url?: string | null;
  line?: number | null;
};

export type SurfaceCtlResult =
  | { ok: true; value: unknown }
  | { ok: false; reason: string; message?: string };

type Handler = {
  priority: number;
  matches: (sessionId: string) => boolean;
  handle: (payload: SurfaceCtlPayload) => Promise<SurfaceCtlResult>;
};

const handlers: Handler[] = [];
let started = false;

export function registerSurfaceCtl(handler: Handler): () => void {
  handlers.push(handler);
  return () => {
    const index = handlers.indexOf(handler);
    if (index >= 0) handlers.splice(index, 1);
  };
}

/** 监听控制通道发来的 surface-ctl。收件箱优先于工作台。 */
export function startSurfaceCtlWatch() {
  if (started || !("__TAURI_INTERNALS__" in window)) return;
  started = true;
  void listen<SurfaceCtlPayload>("surface-ctl", async (event) => {
    const payload = event.payload;
    if (!payload?.requestId) return;
    const sessionId = payload.sessionId ?? "";
    const handler = handlers
      .filter((item) => item.matches(sessionId))
      .sort((left, right) => right.priority - left.priority)[0];
    const result: SurfaceCtlResult = handler
      ? await handler.handle(payload).catch((error: unknown) => ({
          ok: false,
          reason: "failed",
          message: error instanceof Error ? error.message : String(error),
        }))
      : { ok: false, reason: "not_open" };
    await backend.resolveUiOutbound(payload.requestId, result);
  });
}
