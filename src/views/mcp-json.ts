import type { McpInput } from "../domain/types";
import { t } from "../i18n";

export function emptyMcpInput(owner = "multara"): McpInput {
  return {
    name: "",
    previousName: "",
    owner,
    transport: "stdio",
    command: "",
    cwd: "",
    envFile: "",
    args: [],
    url: "",
    envText: "",
    headersText: "",
    needsAuth: false,
    enabled: true,
  };
}

export function mcpInputToJson(form: McpInput, argsText: string): string {
  return JSON.stringify(serverEntry(form, argsText), null, 2);
}

function serverEntry(form: McpInput, argsText: string): Record<string, unknown> {
  const args = parseArgsText(argsText);
  const entry: Record<string, unknown> = {};
  if (form.transport === "http" || form.transport === "sse") {
    entry.type = form.transport;
    if (form.url.trim()) entry.url = form.url.trim();
    const headers = parsePairs(form.headersText, ":");
    if (Object.keys(headers).length) entry.headers = headers;
    if (form.needsAuth) entry.oauth = {};
  } else {
    if (form.command.trim()) entry.command = form.command.trim();
    if (args.length) entry.args = args;
    const env = parsePairs(form.envText, "=");
    if (form.needsAuth && !env.MCP_AUTH_TOKEN) env.MCP_AUTH_TOKEN = "${input:token}";
    if (Object.keys(env).length) entry.env = env;
    if (form.cwd?.trim()) entry.cwd = form.cwd.trim();
    if (form.envFile?.trim()) entry.envFile = form.envFile.trim();
  }
  if (!form.enabled) entry.disabled = true;
  return entry;
}

export function jsonToMcpInputs(raw: string): McpInput[] {
  const parsed = JSON.parse(raw) as unknown;
  if (!parsed || typeof parsed !== "object") throw new Error(t("mcp.json.error.rootObject"));
  const root = parsed as Record<string, unknown>;
  const servers = asRecord(root.mcpServers) ?? asRecord(root.servers) ?? singleServerMap(root);
  const entries = Object.entries(servers);
  if (!entries.length) throw new Error(t("mcp.json.error.noServers"));
  return entries.map(([name, value]) => entryToInput(name, value));
}

function singleServerMap(root: Record<string, unknown>): Record<string, unknown> {
  if (typeof root.command === "string" || typeof root.url === "string" || typeof root.type === "string") {
    return { "": root };
  }
  const nested = Object.entries(root).filter(([, value]) => value && typeof value === "object");
  if (nested.length) return Object.fromEntries(nested);
  return {};
}

function entryToInput(name: string, value: unknown): McpInput {
  const entry = asRecord(value) ?? {};
  const typeName = String(entry.type ?? "");
  const http = typeName === "http" || typeName === "sse" || typeName === "streamable-http" || typeof entry.url === "string";
  const env = asRecord(entry.env) ?? {};
  const headers = asRecord(entry.headers) ?? asRecord(entry.http_headers) ?? {};
  return {
    name,
    previousName: "",
    owner: "",
    transport: typeName === "sse" ? "sse" : http ? "http" : "stdio",
    command: String(entry.command ?? ""),
    cwd: String(entry.cwd ?? ""),
    envFile: String(entry.envFile ?? ""),
    args: Array.isArray(entry.args) ? entry.args.map((item) => String(item)) : [],
    url: String(entry.url ?? entry.serverUrl ?? ""),
    envText: Object.entries(env).map(([key, item]) => `${key}=${item}`).join("\n"),
    headersText: Object.entries(headers).map(([key, item]) => `${key}: ${item}`).join("\n"),
    needsAuth: Boolean(entry.oauth) || String(env.MCP_AUTH_TOKEN ?? "").includes("input:"),
    enabled: entry.disabled !== true && entry.enabled !== false,
  };
}

export function detectedNeedsAuth(form: Pick<McpInput, "transport" | "headersText" | "envText">): boolean {
  if (form.transport === "http" || form.transport === "sse") return !hasStaticAuthorization(form.headersText);
  return form.envText.includes("${input:") || form.envText.includes("${env:");
}

export function parseArgsText(text: string): string[] {
  const trimmed = text.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith("[")) {
    const parsed = JSON.parse(trimmed) as unknown;
    if (!Array.isArray(parsed)) throw new Error(t("mcp.json.error.argsMustArray"));
    return parsed.map((item) => String(item));
  }
  if (trimmed.includes("\n")) {
    return trimmed.split("\n").map((line) => line.trim()).filter(Boolean);
  }
  return trimmed.match(/(?:[^\s"]+|"[^"]*")+/g)?.map((part) => part.replace(/^"|"$/g, "")) ?? [trimmed];
}

export function argsToText(args: string[]): string {
  if (args.some((arg) => /\s/.test(arg))) return JSON.stringify(args);
  return args.join("\n");
}

function hasStaticAuthorization(text: string): boolean {
  const line = text.split("\n").find((item) => item.trim().toLowerCase().startsWith("authorization:"));
  const value = line?.slice(line.indexOf(":") + 1).trim() ?? "";
  if (!value) return false;
  return !value.includes("${input:") && !value.includes("${env:");
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function parsePairs(text: string, separator: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const index = line.indexOf(separator);
    if (index <= 0) continue;
    const key = line.slice(0, index).trim();
    const value = line.slice(index + separator.length).trim();
    if (key) out[key] = value;
  }
  return out;
}
