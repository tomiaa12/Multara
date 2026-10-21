import type { Plugin } from "vite";
import type { IncomingMessage, ServerResponse } from "node:http";

interface Command {
  id: number;
  payload: Record<string, unknown>;
}

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

export function e2eVitePlugin(): Plugin {
  let nextId = 1;
  const queued: Command[] = [];
  const waitingRuns = new Map<number, (value: unknown) => void>();

  return {
    name: "multara-e2e-bridge",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const url = req.url?.split("?")[0] ?? "";
        if (!url.startsWith("/__e2e/")) {
          next();
          return;
        }

        if (url === "/__e2e/next" && req.method === "GET") {
          const command = queued.shift();
          json(res, 200, command ?? { id: 0, payload: { action: "idle" } });
          return;
        }

        if (url === "/__e2e/result" && req.method === "POST") {
          const body = JSON.parse((await readBody(req)) || "{}") as {
            id?: number;
            ok?: boolean;
            result?: unknown;
            error?: string;
          };
          const finish = body.id != null ? waitingRuns.get(body.id) : undefined;
          if (finish) {
            waitingRuns.delete(body.id!);
            finish(body);
          }
          json(res, 200, { ok: true });
          return;
        }

        if (url === "/__e2e/run" && req.method === "POST") {
          const payload = JSON.parse((await readBody(req)) || "{}") as Record<string, unknown>;
          const command = { id: nextId, payload };
          nextId += 1;
          const result = new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
              waitingRuns.delete(command.id);
              reject(new Error("e2e command timed out"));
            }, 45_000);
            waitingRuns.set(command.id, (value) => {
              clearTimeout(timer);
              resolve(value);
            });
          });
          queued.push(command);
          try {
            json(res, 200, await result);
          } catch (cause) {
            json(res, 504, { ok: false, error: cause instanceof Error ? cause.message : String(cause) });
          }
          return;
        }

        json(res, 404, { ok: false, error: "not found" });
      });
    },
  };
}
