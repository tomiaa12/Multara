import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function sleep(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function inTauri() {
  return "__TAURI_INTERNALS__" in window;
}

/**
 * 仅在 Tauri WebView 里跑：替浏览器 DEV 执行真实 invoke。
 * Chrome 页只走 /__multara/invoke，不会进这个 loop。
 */
async function loop() {
  while (true) {
    try {
      const response = await fetch("/__multara/next", { cache: "no-store" });
      if (!response.ok) {
        await sleep(800);
        continue;
      }
      const command = (await response.json()) as {
        id?: number;
        cmd?: string;
        args?: Record<string, unknown>;
      };
      if (!command?.id || !command.cmd) {
        await sleep(120);
        continue;
      }
      let body: Record<string, unknown>;
      try {
        body = {
          id: command.id,
          ok: true,
          result: await invoke(command.cmd, command.args ?? {}),
        };
      } catch (cause) {
        body = {
          id: command.id,
          ok: false,
          error: cause instanceof Error ? cause.message : String(cause),
        };
      }
      await fetch("/__multara/result", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    } catch {
      await sleep(800);
    }
  }
}

const FORWARDED_EVENTS = ["terminal-ctl", "terminal-data", "terminal-exit"];

/** 把桌面窗口收到的界面事件交给同一套 Vite 上的浏览器。 */
function forwardEvents(names: string[]) {
  const queue: Array<{ event: string; payload: unknown }> = [];
  let timer = 0;
  const flush = () => {
    timer = 0;
    if (!queue.length) return;
    const batch = queue.splice(0, queue.length);
    void fetch("/__multara/push-events", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(batch),
    }).catch(() => undefined);
  };
  for (const name of names) {
    void listen(name, (event) => {
      queue.push({ event: name, payload: event.payload });
      if (!timer) timer = window.setTimeout(flush, 40);
    });
  }
}

if (import.meta.env.DEV && inTauri()) {
  void loop();
  // 浏览器开发模式还要把 terminal-ctl 交出去，因为隐藏窗口自己不处理。
  // tauri:dev 的桌面窗口会自己处理 terminal-ctl，只把输出转给旁边的浏览器。
  const names = import.meta.env.MULTARA_BROWSER_DEV
    ? FORWARDED_EVENTS
    : ["terminal-data", "terminal-exit"];
  forwardEvents(names);
}
