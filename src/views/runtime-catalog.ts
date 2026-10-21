import { cliEntry, runtimeEntries, type CliProtocol, type CliUrlSuffix } from "../domain/cli-catalog";

export type RuntimeFamilyId = string;

export type RuntimeProtocol = CliProtocol;

export interface RuntimeFamily {
  id: RuntimeFamilyId;
  command: string;
  protocol: RuntimeProtocol;
  supported: boolean;
  gateway: boolean;
  /** Whether the gateway base URL should include /v1. */
  urlSuffix: CliUrlSuffix;
}

/** 运行时列表来自 `shared/cli-catalog.json`，后续加 CLI 只改那份配置。 */
export const RUNTIME_FAMILIES: RuntimeFamily[] = runtimeEntries().map((item) => ({
  id: item.id,
  command: item.command ?? item.id,
  protocol: item.protocol ?? "openai",
  supported: item.supported !== false,
  gateway: Boolean(item.gateway),
  urlSuffix: item.urlSuffix ?? "v1",
}));

export const PROFILE_FAMILIES = RUNTIME_FAMILIES;
export const GATEWAY_FAMILIES = RUNTIME_FAMILIES.filter((item) => item.gateway);

export function familyOf(id: string): RuntimeFamily {
  const found = RUNTIME_FAMILIES.find((item) => item.id === id);
  if (found) return found;
  const fromCatalog = cliEntry(id);
  if (fromCatalog?.command && fromCatalog.protocol) {
    return {
      id: fromCatalog.id,
      command: fromCatalog.command,
      protocol: fromCatalog.protocol,
      supported: fromCatalog.supported !== false,
      gateway: Boolean(fromCatalog.gateway),
      urlSuffix: fromCatalog.urlSuffix ?? "v1",
    };
  }
  return RUNTIME_FAMILIES[0];
}

export function protocolFor(provider: string): RuntimeProtocol {
  return familyOf(provider).protocol;
}

export type CommandLineParseError =
  | "empty"
  | "unclosed_quote"
  | "trailing_escape"
  | "shell_syntax"
  | "shell_expansion";

export type ParsedCommandLine =
  | { ok: true; commandName: string; fixedArgs: string[] }
  | { ok: false; error: CommandLineParseError };

const SHELL_CONTROL_CHARS = new Set(["|", ">", "<", ";", "&"]);

export function parseCommandLine(input: string): ParsedCommandLine {
  const line = input.trim();
  if (!line) return { ok: false, error: "empty" };

  const tokens: string[] = [];
  let token = "";
  let quote: "'" | '"' | null = null;
  let tokenStarted = false;

  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i] ?? "";
    const next = line[i + 1] ?? "";

    if (quote == null && /\s/.test(ch)) {
      if (tokenStarted) {
        tokens.push(token);
        token = "";
        tokenStarted = false;
      }
      continue;
    }

    if (quote == null) {
      if (ch === "`" || ch === "$") {
        return { ok: false, error: ch === "$" ? "shell_expansion" : "shell_syntax" };
      }
      if (SHELL_CONTROL_CHARS.has(ch)) return { ok: false, error: "shell_syntax" };
      if (ch === "\\" && next) {
        token += next;
        tokenStarted = true;
        i += 1;
        continue;
      }
      if (ch === "\\") return { ok: false, error: "trailing_escape" };
      if (ch === "'" || ch === '"') {
        quote = ch;
        tokenStarted = true;
        continue;
      }
      token += ch;
      tokenStarted = true;
      continue;
    }

    if (ch === quote) {
      quote = null;
      tokenStarted = true;
      continue;
    }
    if (quote === '"' && ch === "\\" && next) {
      token += next;
      tokenStarted = true;
      i += 1;
      continue;
    }
    if (quote === '"' && ch === "\\") return { ok: false, error: "trailing_escape" };
    if (quote !== "'" && (ch === "`" || ch === "$")) {
      return { ok: false, error: ch === "$" ? "shell_expansion" : "shell_syntax" };
    }
    token += ch;
    tokenStarted = true;
  }

  if (quote != null) return { ok: false, error: "unclosed_quote" };
  if (tokenStarted) tokens.push(token);
  if (tokens.length === 0 || !tokens[0]?.trim()) return { ok: false, error: "empty" };
  return { ok: true, commandName: tokens[0], fixedArgs: tokens.slice(1) };
}

export function formatCommandLine(commandName: string, fixedArgs: string[]): string {
  return [commandName, ...fixedArgs].filter((part) => part != null).map(quoteArg).join(" ");
}

function quoteArg(arg: string): string {
  if (arg === "") return '""';
  if (!/[\s"'\\|<>;&`$]/.test(arg)) return arg;
  return `"${arg.replace(/(["\\$`])/g, "\\$1")}"`;
}

export interface GatewayDraft {
  provider: RuntimeFamilyId;
  name: string;
  commandLine: string;
  description: string;
  apiUrl: string;
  apiKey: string;
  modelsText: string;
  wireApi: "responses" | "chat";
  keyStyle: "bearer" | "x-api-key";
  envText: string;
  maxConcurrent: number;
}

export function gatewayDraftFor(provider: RuntimeFamilyId, previous?: Partial<GatewayDraft>): GatewayDraft {
  const spec = familyOf(provider);
  return {
    provider,
    name: previous?.name ?? "",
    commandLine: spec.command,
    description: previous?.description ?? "",
    apiUrl: "",
    apiKey: "",
    modelsText: "",
    wireApi: "responses",
    keyStyle: "bearer",
    envText: "",
    maxConcurrent: previous?.maxConcurrent ?? 2,
  };
}

export function parseEnvText(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const index = line.indexOf("=");
    if (index <= 0) continue;
    const key = line.slice(0, index).trim();
    if (!key || MANAGED_ENV_KEYS.has(key)) continue;
    out[key] = line.slice(index + 1).trim();
  }
  return out;
}

const MANAGED_ENV_KEYS = new Set([
  "ANTHROPIC_BASE_URL",
  "ANTHROPIC_API_KEY",
  "ANTHROPIC_AUTH_TOKEN",
  "ANTHROPIC_MODEL",
  "OPENAI_BASE_URL",
  "OPENAI_API_KEY",
  "OPENAI_MODEL",
  "GOOGLE_GEMINI_BASE_URL",
  "GEMINI_API_KEY",
  "GEMINI_MODEL",
  "MULTARA_GATEWAY_KEY",
  "OPENCODE_CONFIG_CONTENT",
]);

export function envTextFrom(record: Record<string, string>): string {
  return Object.entries(record)
    .filter(([key]) => !MANAGED_ENV_KEYS.has(key))
    .map(([key, value]) => `${key}=${value}`)
    .join("\n");
}

export function gatewayHost(url: string): string {
  const trimmed = url.trim();
  if (!trimmed) return "";
  try {
    return new URL(trimmed).host;
  } catch {
    return trimmed.replace(/^https?:\/\//, "").split("/")[0] ?? trimmed;
  }
}
