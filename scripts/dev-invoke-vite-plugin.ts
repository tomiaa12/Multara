import type { Plugin } from "vite";
import type { IncomingMessage, ServerResponse } from "node:http";

interface InvokeCommand {
  id: number;
  cmd: string;
  args: Record<string, unknown>;
}

const WORKER_STALE_MS = 4_000;
const INVOKE_TIMEOUT_MS = 180_000;

function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

function json(res: ServerResponse, status: number, body: unknown) {
  res.statusCode = status;
  res.setHeader("Content-Type", "application/json");
  res.setHeader("Cache-Control", "no-store");
  res.end(JSON.stringify(body));
}

/**
 * 浏览器 DEV 调 Rust：Chrome POST /__multara/invoke →
 * Tauri WebView worker 轮询 /__multara/next 执行真实 invoke → 回 /__multara/result。
 */
export function devInvokeVitePlugin(): Plugin {
  let nextId = 1;
  const queued: InvokeCommand[] = [];
  const waiting = new Map<number, (value: unknown) => void>();
  const events: Array<{ event: string; payload: unknown }> = [];
  let lastWorkerAt = 0;

  return {
    name: "multara-dev-invoke-bridge",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const url = req.url?.split("?")[0] ?? "";
        if (!url.startsWith("/__multara/")) {
          next();
          return;
        }

        if (url === "/__multara/health" && req.method === "GET") {
          json(res, 200, {
            ok: true,
            worker: lastWorkerAt > 0 && Date.now() - lastWorkerAt < WORKER_STALE_MS,
          });
          return;
        }

        if (url === "/__multara/next" && req.method === "GET") {
          lastWorkerAt = Date.now();
          const command = queued.shift();
          json(res, 200, command ?? { id: 0, cmd: "", args: {} });
          return;
        }

        if (url === "/__multara/result" && req.method === "POST") {
          const body = JSON.parse((await readBody(req)) || "{}") as {
            id?: number;
            ok?: boolean;
            result?: unknown;
            error?: string;
          };
          const finish = body.id != null ? waiting.get(body.id) : undefined;
          if (finish) {
            waiting.delete(body.id!);
            finish(body);
          }
          json(res, 200, { ok: true });
          return;
        }

        if (url === "/__multara/push-events" && req.method === "POST") {
          const body = JSON.parse((await readBody(req)) || "[]") as Array<{
            event?: string;
            payload?: unknown;
          }>;
          if (Array.isArray(body)) {
            for (const item of body) {
              if (!item?.event) continue;
              events.push({ event: item.event, payload: item.payload });
            }
            if (events.length > 400) events.splice(0, events.length - 400);
          }
          json(res, 200, { ok: true });
          return;
        }

        if (url === "/__multara/events" && req.method === "GET") {
          const batch = events.splice(0, events.length);
          json(res, 200, batch);
          return;
        }

        if (url === "/__multara/invoke" && req.method === "POST") {
          if (lastWorkerAt === 0 || Date.now() - lastWorkerAt >= WORKER_STALE_MS) {
            json(res, 503, { ok: false, error: "Tauri invoke worker not ready" });
            return;
          }
          const payload = JSON.parse((await readBody(req)) || "{}") as {
            cmd?: string;
            args?: Record<string, unknown>;
          };
          const cmd = String(payload.cmd ?? "").trim();
          if (!cmd) {
            json(res, 400, { ok: false, error: "missing cmd" });
            return;
          }
          const command: InvokeCommand = {
            id: nextId,
            cmd,
            args: payload.args && typeof payload.args === "object" ? payload.args : {},
          };
          nextId += 1;
          const result = new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
              waiting.delete(command.id);
              reject(new Error(`invoke ${cmd} timed out`));
            }, INVOKE_TIMEOUT_MS);
            waiting.set(command.id, (value) => {
              clearTimeout(timer);
              resolve(value);
            });
          });
          queued.push(command);
          try {
            json(res, 200, await result);
          } catch (cause) {
            json(res, 504, {
              ok: false,
              error: cause instanceof Error ? cause.message : String(cause),
            });
          }
          return;
        }

        json(res, 404, { ok: false, error: "not found" });
      });
    },
  };
}
