import { listen } from "@tauri-apps/api/event";
import { monaco } from "../../monaco";
import * as backend from "../../services/backend";
import type { LocatedFile } from "../../components/workbench/workspace-file-location";
import { extensionOf } from "../definition-heuristics";
import { locateFsPath, locateModel, workspaceModelUri } from "../workspace-models";
import {
  fileUriFromFsPath,
  fsPathFromFileUri,
  joinFsPath,
  normalizeDefinitionResult,
  replyToServerRequest,
  type JsonRpcMessage,
} from "./protocol";

type ServerChoice = { server: string; languageId: string };

const SERVERS_BY_EXTENSION: Record<string, ServerChoice> = {
  rs: { server: "rust", languageId: "rust" },
  py: { server: "python", languageId: "python" },
  pyi: { server: "python", languageId: "python" },
  go: { server: "go", languageId: "go" },
  c: { server: "cpp", languageId: "c" },
  h: { server: "cpp", languageId: "c" },
  cc: { server: "cpp", languageId: "cpp" },
  cpp: { server: "cpp", languageId: "cpp" },
  cxx: { server: "cpp", languageId: "cpp" },
  hpp: { server: "cpp", languageId: "cpp" },
  hh: { server: "cpp", languageId: "cpp" },
  hxx: { server: "cpp", languageId: "cpp" },
  vue: { server: "vue", languageId: "vue" },
  php: { server: "php", languageId: "php" },
  rb: { server: "ruby", languageId: "ruby" },
};

const INITIALIZE_TIMEOUT_MS = 60_000;
const REQUEST_TIMEOUT_MS = 10_000;
const CHANGE_DEBOUNCE_MS = 150;
const MAX_RESTARTS = 3;

export function serverForPath(path: string): ServerChoice | null {
  return SERVERS_BY_EXTENSION[extensionOf(path)] ?? null;
}

type OpenDocument = {
  uri: string;
  version: number;
  timer: number | null;
  disposables: monaco.IDisposable[];
};

type Pending = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: number;
};

class LspConnection {
  private nextId = 1;
  private outgoing: Promise<void> = Promise.resolve();
  private readonly pending = new Map<number, Pending>();
  private readonly documents = new Map<string, OpenDocument>();
  private readonly workspaceFolders: Array<{ uri: string; name: string }>;
  private closed = false;

  constructor(
    readonly key: string,
    readonly workspaceId: string,
    readonly info: backend.LspServerInfo,
  ) {
    this.workspaceFolders = [{ uri: fileUriFromFsPath(info.rootPath), name: workspaceId }];
  }

  async initialize() {
    await this.request(
      "initialize",
      {
        processId: null,
        clientInfo: { name: "Multara" },
        rootPath: this.info.rootPath,
        rootUri: this.workspaceFolders[0].uri,
        workspaceFolders: this.workspaceFolders,
        initializationOptions: this.info.initializationOptions ?? undefined,
        capabilities: {
          general: { positionEncodings: ["utf-16"] },
          textDocument: {
            synchronization: { dynamicRegistration: false, didSave: false, willSave: false },
            definition: { dynamicRegistration: false, linkSupport: true },
          },
          workspace: { configuration: true, workspaceFolders: true },
          window: { workDoneProgress: false },
        },
      },
      INITIALIZE_TIMEOUT_MS,
    );
    this.notify("initialized", {});
  }

  open(model: monaco.editor.ITextModel, path: string, languageId: string) {
    const key = model.uri.toString();
    if (this.closed || this.documents.has(key)) return;
    const document: OpenDocument = {
      uri: fileUriFromFsPath(joinFsPath(this.info.rootPath, path)),
      version: 1,
      timer: null,
      disposables: [],
    };
    this.documents.set(key, document);
    this.notify("textDocument/didOpen", {
      textDocument: { uri: document.uri, languageId, version: document.version, text: model.getValue() },
    });
    document.disposables.push(
      model.onDidChangeContent(() => {
        if (document.timer != null) window.clearTimeout(document.timer);
        document.timer = window.setTimeout(() => this.sendChange(model, document), CHANGE_DEBOUNCE_MS);
      }),
      model.onWillDispose(() => this.close(key)),
    );
  }

  async definition(model: monaco.editor.ITextModel, position: monaco.Position) {
    const document = this.documents.get(model.uri.toString());
    if (!document) return [];
    if (document.timer != null) this.sendChange(model, document);
    const result = await this.request("textDocument/definition", {
      textDocument: { uri: document.uri },
      position: { line: position.lineNumber - 1, character: position.column - 1 },
    });
    return normalizeDefinitionResult(result);
  }

  receive(message: JsonRpcMessage) {
    if (message.method && message.id != null) {
      const reply = replyToServerRequest(message.method, message.params, this.workspaceFolders);
      this.send({ jsonrpc: "2.0", id: message.id, ...reply });
      return;
    }
    if (message.method || typeof message.id !== "number") return;
    const pending = this.pending.get(message.id);
    if (!pending) return;
    this.pending.delete(message.id);
    window.clearTimeout(pending.timer);
    if (message.error) pending.reject(new Error(message.error.message));
    else pending.resolve(message.result);
  }

  dispose() {
    if (this.closed) return;
    this.closed = true;
    for (const pending of this.pending.values()) {
      window.clearTimeout(pending.timer);
      pending.reject(new Error("语言服务器已退出"));
    }
    this.pending.clear();
    for (const document of this.documents.values()) {
      if (document.timer != null) window.clearTimeout(document.timer);
      document.disposables.forEach((item) => item.dispose());
    }
    this.documents.clear();
  }

  private close(key: string) {
    const document = this.documents.get(key);
    if (!document) return;
    this.documents.delete(key);
    if (document.timer != null) window.clearTimeout(document.timer);
    document.disposables.forEach((item) => item.dispose());
    this.notify("textDocument/didClose", { textDocument: { uri: document.uri } });
  }

  private sendChange(model: monaco.editor.ITextModel, document: OpenDocument) {
    if (document.timer != null) window.clearTimeout(document.timer);
    document.timer = null;
    if (model.isDisposed()) return;
    document.version += 1;
    this.notify("textDocument/didChange", {
      textDocument: { uri: document.uri, version: document.version },
      contentChanges: [{ text: model.getValue() }],
    });
  }

  private request(method: string, params: unknown, timeoutMs = REQUEST_TIMEOUT_MS): Promise<unknown> {
    if (this.closed) return Promise.reject(new Error("语言服务器已退出"));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = window.setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method} 超时`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.send({ jsonrpc: "2.0", id, method, params });
    });
  }

  private notify(method: string, params: unknown) {
    if (!this.closed) this.send({ jsonrpc: "2.0", method, params });
  }

  /** 串行发送：didOpen/didChange 必须先于依赖它们的请求到达服务器。 */
  private send(message: JsonRpcMessage) {
    const text = JSON.stringify(message);
    this.outgoing = this.outgoing
      .then(() => backend.lspSend(this.info.id, text))
      .catch(() => undefined);
  }
}

/** 按（工作区, 语言）懒启动语言服务器，并把该语言的模型同步过去。 */
class LanguageServers {
  private readonly connections = new Map<string, Promise<LspConnection | null>>();
  private readonly live = new Map<string, LspConnection>();
  private readonly restarts = new Map<string, number>();
  private started = false;

  start() {
    if (this.started || !backend.isTauriRuntime()) return;
    this.started = true;
    void listen<{ id: string; message: string }>("lsp-message", (event) => {
      const connection = this.live.get(event.payload.id);
      if (!connection) return;
      try {
        connection.receive(JSON.parse(event.payload.message) as JsonRpcMessage);
      } catch {
        // 非 JSON 输出忽略
      }
    });
    void listen<{ id: string }>("lsp-exit", (event) => this.forget(event.payload.id, true));
    monaco.editor.onDidCreateModel((model) => this.track(model));
    for (const model of monaco.editor.getModels()) this.track(model);
  }

  /** 工作区移除后停掉它的语言服务器。 */
  retain(workspaceIds: Set<string>) {
    for (const connection of [...this.live.values()]) {
      if (workspaceIds.has(connection.workspaceId)) continue;
      void backend.lspStop(connection.info.id);
      this.forget(connection.info.id, false);
    }
  }

  async definition(
    model: monaco.editor.ITextModel,
    position: monaco.Position,
  ): Promise<monaco.languages.Location[] | null> {
    const target = locateModel(model.uri);
    const choice = target && serverForPath(target.path);
    if (!target || !choice) return null;
    const connection = await this.connection(target.workspaceId, choice.server);
    if (!connection) return null;
    connection.open(model, target.path, choice.languageId);
    const targets = await connection.definition(model, position);
    const locations: monaco.languages.Location[] = [];
    for (const item of targets) {
      const path = fsPathFromFileUri(item.uri);
      const located: LocatedFile | null = path ? locateFsPath(path) : null;
      if (!located) continue;
      locations.push({
        uri: workspaceModelUri(located.workspaceId, located.path),
        range: new monaco.Range(
          item.range.start.line + 1,
          item.range.start.character + 1,
          item.range.end.line + 1,
          item.range.end.character + 1,
        ),
      });
    }
    return locations;
  }

  private track(model: monaco.editor.ITextModel) {
    const target = locateModel(model.uri);
    const choice = target && serverForPath(target.path);
    if (!target || !choice) return;
    void this.connection(target.workspaceId, choice.server).then((connection) =>
      connection?.open(model, target.path, choice.languageId),
    );
  }

  private connection(workspaceId: string, server: string): Promise<LspConnection | null> {
    const key = `${workspaceId}\0${server}`;
    const existing = this.connections.get(key);
    if (existing) return existing;
    if ((this.restarts.get(key) ?? 0) >= MAX_RESTARTS) return Promise.resolve(null);
    const launching = this.launch(key, workspaceId, server).catch(() => {
      this.connections.delete(key);
      this.restarts.set(key, (this.restarts.get(key) ?? 0) + 1);
      return null;
    });
    this.connections.set(key, launching);
    return launching;
  }

  private async launch(key: string, workspaceId: string, server: string) {
    const info = await backend.lspStart(workspaceId, server);
    if (!info) return null;
    const connection = new LspConnection(key, workspaceId, info);
    this.live.set(info.id, connection);
    try {
      await connection.initialize();
    } catch (error) {
      void backend.lspStop(info.id);
      this.forget(info.id, false);
      throw error;
    }
    for (const model of monaco.editor.getModels()) {
      const target = locateModel(model.uri);
      const choice = target && serverForPath(target.path);
      if (target?.workspaceId === workspaceId && choice?.server === server) {
        connection.open(model, target.path, choice.languageId);
      }
    }
    return connection;
  }

  private forget(id: string, crashed: boolean) {
    const connection = this.live.get(id);
    if (!connection) return;
    this.live.delete(id);
    connection.dispose();
    this.connections.delete(connection.key);
    if (crashed) this.restarts.set(connection.key, (this.restarts.get(connection.key) ?? 0) + 1);
  }
}

export const languageServers = new LanguageServers();
