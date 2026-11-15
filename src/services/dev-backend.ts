import { invoke as tauriInvoke } from "@tauri-apps/api/core";

let bridgeLive = false;

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 桌面 WebView，或浏览器经 Vite 桥连上同一套 Rust。 */
export function hasBackend(): boolean {
  return inTauri() || bridgeLive;
}

/**
 * 浏览器 DEV：探测隐藏 WebView 里的 invoke worker。
 * `pnpm dev`（MULTARA_BROWSER_DEV）会等 Rust 编完；`pnpm dev:web` 很快回落 preview。
 */
export async function connectDevBackend(timeoutMs?: number): Promise<boolean> {
  if (inTauri()) return true;
  if (!import.meta.env.DEV || import.meta.env.MODE === "test") return false;
  const expectBridge =
    (import.meta.env as ImportMeta["env"] & { MULTARA_BROWSER_DEV?: boolean }).MULTARA_BROWSER_DEV === true;
  const budget = timeoutMs ?? (expectBridge ? 180_000 : 2_500);
  const started = Date.now();
  while (Date.now() - started < budget) {
    try {
      const response = await fetch("/__multara/health", { cache: "no-store" });
      if (!response.ok) return false;
      const body = (await response.json()) as { worker?: boolean };
      if (body.worker) {
        bridgeLive = true;
        return true;
      }
    } catch {
      // 无 Vite 中间件（单测 / 非 serve）
      return false;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  return false;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri()) return tauriInvoke<T>(cmd, args);
  if (!bridgeLive) throw new Error(`dev bridge offline: ${cmd}`);
  const response = await fetch("/__multara/invoke", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ cmd, args: args ?? {} }),
  });
  const body = (await response.json().catch(() => ({}))) as {
    ok?: boolean;
    result?: T;
    error?: string;
  };
  if (response.status === 504) {
    throw new Error(body.error || `invoke ${cmd} timed out`);
  }
  if (!response.ok || body.ok === false) {
    throw new Error(body.error || `invoke ${cmd} failed`);
  }
  return body.result as T;
}
