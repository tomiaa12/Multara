import { normalizeDriveLetter } from "../definition-heuristics";

export type LspPosition = { line: number; character: number };
export type LspRange = { start: LspPosition; end: LspPosition };
export type LspDefinitionTarget = { uri: string; range: LspRange };

type Location = { uri: string; range: LspRange };
type LocationLink = { targetUri: string; targetRange: LspRange; targetSelectionRange?: LspRange };

export type JsonRpcMessage = {
  jsonrpc?: "2.0";
  id?: number | string | null;
  method?: string;
  params?: unknown;
  result?: unknown;
  error?: { code: number; message: string };
};

/** `textDocument/definition` 可以返回 Location、Location[]、LocationLink[] 或 null。 */
export function normalizeDefinitionResult(result: unknown): LspDefinitionTarget[] {
  const items = Array.isArray(result) ? result : result ? [result] : [];
  const targets: LspDefinitionTarget[] = [];
  for (const item of items) {
    if (!item || typeof item !== "object") continue;
    if ("targetUri" in item) {
      const link = item as LocationLink;
      targets.push({ uri: link.targetUri, range: link.targetSelectionRange ?? link.targetRange });
    } else if ("uri" in item && "range" in item) {
      const location = item as Location;
      targets.push({ uri: location.uri, range: location.range });
    }
  }
  return targets;
}

export function fsPathFromFileUri(uri: string): string | null {
  if (!uri.toLowerCase().startsWith("file://")) return null;
  const rest = uri.slice("file://".length);
  const slash = rest.indexOf("/");
  const authority = slash < 0 ? rest : rest.slice(0, slash);
  let path: string;
  try {
    path = decodeURIComponent(slash < 0 ? "/" : rest.slice(slash));
  } catch {
    return null;
  }
  if (/^\/[A-Za-z]:/.test(path)) path = path.slice(1);
  if (authority && authority !== "localhost") path = `//${authority}${path}`;
  return normalizeDriveLetter(path);
}

export function fileUriFromFsPath(path: string): string {
  const normalized = normalizeDriveLetter(path);
  const segments = normalized.split("/").map((segment, index) =>
    index === 0 && /^[A-Za-z]:$/.test(segment) ? segment : encodeURIComponent(segment),
  );
  const joined = segments.join("/");
  return joined.startsWith("/") ? `file://${joined}` : `file:///${joined}`;
}

export function joinFsPath(root: string, relative: string): string {
  return `${normalizeDriveLetter(root).replace(/\/+$/, "")}/${relative}`;
}

/** LSP `TextDocumentSyncKind.Incremental`。 */
export const TEXT_DOCUMENT_SYNC_INCREMENTAL = 2;

/** `initialize` 结果里的 `textDocumentSync` 可以是数字，也可以是带 `change` 的对象。 */
export function supportsIncrementalSync(initializeResult: unknown) {
  const sync = (initializeResult as { capabilities?: { textDocumentSync?: number | { change?: number } } } | null)
    ?.capabilities?.textDocumentSync;
  return (typeof sync === "number" ? sync : sync?.change) === TEXT_DOCUMENT_SYNC_INCREMENTAL;
}

/** 服务器反向请求里不处理的方法也必须回复，否则有的服务器会一直等。 */
export function replyToServerRequest(
  method: string,
  params: unknown,
  workspaceFolders: Array<{ uri: string; name: string }>,
): { result: unknown } | { error: { code: number; message: string } } {
  switch (method) {
    case "workspace/configuration": {
      const items = (params as { items?: unknown[] } | undefined)?.items ?? [];
      return { result: items.map(() => null) };
    }
    case "workspace/workspaceFolders":
      return { result: workspaceFolders };
    case "window/workDoneProgress/create":
    case "client/registerCapability":
    case "client/unregisterCapability":
    case "window/showMessageRequest":
    case "window/showDocument":
      return { result: null };
    case "workspace/applyEdit":
      return { result: { applied: false } };
    default:
      return { error: { code: -32601, message: `Unhandled method ${method}` } };
  }
}
