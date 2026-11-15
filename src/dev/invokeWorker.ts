import { invoke } from "@tauri-apps/api/core";

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

if (import.meta.env.DEV && inTauri()) {
  void loop();
}
