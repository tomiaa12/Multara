// MCP Apps 的 postMessage 桥（方案 §7.3）：页面 ↔ Multara 的 JSON-RPC。
// 只在 MCP App 的 iframe 上用；Canvas 页面没有任何桥（UiFrame 不引用这里）。
// 页面写进会话的东西照 VS Code 很保守：ui/message 只填输入框，ui/update-model-context 只挂附件。

export const MCP_APPS_PROTOCOL_VERSION = "2026-01-26";
const METHOD_NOT_FOUND = -32601;
const MIN_HEIGHT = 38;

export type JsonRpcId = string | number;

interface JsonRpcMessage {
  jsonrpc?: string;
  id?: JsonRpcId;
  method?: string;
  params?: unknown;
}

export interface McpAppHost {
  hostInfo: { name: string; version: string };
  csp: unknown;
  clipboardWrite: boolean;
  /** 工具调用参数（tool_use.input） */
  toolInput(): unknown;
  /** 原始 CallToolResult；工具还没完成时 undefined */
  toolResult(): unknown;
  hostContext(): Record<string, unknown>;
  /** 转给 MCP server：tools/call、resources/read、notifications/message */
  call(method: string, params: unknown): Promise<unknown>;
  /** ui/message：输入框为空时填进去，用户自己按发送 */
  fillComposer(text: string): { ok: boolean };
  /** ui/update-model-context：挂成下一轮的上下文附件（同一 App 只保留最新一份） */
  attachContext(text: string | null, data: unknown): void;
  openLink(url: string): Promise<boolean>;
  setHeight(height: number): void;
  maxHeight(): number;
}

type Post = (message: unknown) => void;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** 把 ui/message、ui/update-model-context 的 content 块压成文字 */
export function contentText(content: unknown): string {
  if (!Array.isArray(content)) return "";
  return content
    .map((block) => (isRecord(block) && block.type === "text" && typeof block.text === "string" ? block.text : ""))
    .filter(Boolean)
    .join("\n");
}

export class McpAppBridge {
  private initialized = false;
  private toolInputSent = false;
  private toolResultSent = false;
  private cancelled = new Set<JsonRpcId>();
  private disposed = false;

  constructor(
    private readonly post: Post,
    private readonly host: McpAppHost,
  ) {}

  dispose() {
    this.disposed = true;
  }

  /** 页面发来的一条消息 */
  async handle(raw: unknown): Promise<void> {
    if (this.disposed || !isRecord(raw)) return;
    const message = raw as JsonRpcMessage;
    if (typeof message.method !== "string") return;
    const isRequest = message.id !== undefined && message.id !== null;
    if (!isRequest) {
      await this.handleNotification(message.method, message.params);
      return;
    }
    const id = message.id as JsonRpcId;
    try {
      const result = await this.handleRequest(message.method, message.params);
      if (this.cancelled.delete(id) || this.disposed) return;
      this.post({ jsonrpc: "2.0", id, result: result ?? {} });
      if (message.method === "ui/initialize") this.afterInitialize();
    } catch (cause) {
      if (this.cancelled.delete(id) || this.disposed) return;
      const code = isRecord(cause) && typeof cause.code === "number" ? cause.code : -32000;
      const text = cause instanceof Error ? cause.message : isRecord(cause) && typeof cause.message === "string" ? cause.message : String(cause);
      this.post({ jsonrpc: "2.0", id, error: { code, message: text } });
    }
  }

  private async handleRequest(method: string, params: unknown): Promise<unknown> {
    const record = isRecord(params) ? params : {};
    switch (method) {
      case "ui/initialize":
        // 重复 initialize 视为页面重载：重新推 tool-input / tool-result
        this.initialized = false;
        this.toolInputSent = false;
        this.toolResultSent = false;
        this.cancelled.clear();
        return {
          protocolVersion: MCP_APPS_PROTOCOL_VERSION,
          hostInfo: this.host.hostInfo,
          hostCapabilities: {
            openLinks: {},
            serverTools: { listChanged: true },
            serverResources: { listChanged: true },
            logging: {},
            sandbox: {
              csp: this.host.csp ?? undefined,
              permissions: this.host.clipboardWrite ? { clipboardWrite: {} } : {},
            },
            updateModelContext: { image: {}, resourceLink: {}, resource: {}, structuredContent: {} },
            downloadFile: {},
          },
          hostContext: this.host.hostContext(),
        };
      case "ping":
        return {};
      case "tools/call":
      case "resources/read":
        return this.host.call(method, params);
      case "ui/message": {
        const text = contentText(record.content);
        if (!text) return { isError: true };
        return this.host.fillComposer(text).ok ? {} : { isError: true };
      }
      case "ui/update-model-context": {
        const text = contentText(record.content) || null;
        const data = record.structuredContent ?? null;
        if (!text && data === null) return { isError: true };
        this.host.attachContext(text, data);
        return {};
      }
      case "ui/open-link": {
        const url = typeof record.url === "string" ? record.url : "";
        if (!/^https?:\/\//i.test(url)) return { isError: true };
        return (await this.host.openLink(url)) ? {} : { isError: true };
      }
      case "ui/request-display-mode":
        return { mode: "inline" };
      case "ui/download-file":
        // 下载区还没做：明确告诉页面不支持，而不是假装成功
        return { isError: true };
      default:
        throw { code: METHOD_NOT_FOUND, message: `Method not found: ${method}` };
    }
  }

  private async handleNotification(method: string, params: unknown) {
    const record = isRecord(params) ? params : {};
    switch (method) {
      case "ui/notifications/initialized":
        this.initialized = true;
        return;
      case "ui/notifications/size-changed": {
        const height = typeof record.height === "number" ? record.height : Number(record.height);
        if (Number.isFinite(height)) {
          this.host.setHeight(Math.max(MIN_HEIGHT, Math.min(this.host.maxHeight(), Math.ceil(height))));
        }
        return;
      }
      case "notifications/cancelled": {
        const id = record.requestId;
        if (typeof id === "string" || typeof id === "number") this.cancelled.add(id);
        return;
      }
      case "notifications/message":
        await this.host.call("notifications/message", params).catch(() => undefined);
        return;
      default:
        // notifications/tools/list_changed 等：宿主下次建连会重新列工具，这里不用处理
        return;
    }
  }

  /** initialize 应答之后才推参数；工具已完成就接着推结果 */
  private afterInitialize() {
    if (this.disposed) return;
    this.post({
      jsonrpc: "2.0",
      method: "ui/notifications/tool-input",
      params: { arguments: this.host.toolInput() },
    });
    this.toolInputSent = true;
    this.notifyToolResult();
  }

  /** 工具结果到了（或 initialize 之后已有结果）：推一次 */
  notifyToolResult() {
    if (this.disposed || !this.toolInputSent || this.toolResultSent) return;
    const result = this.host.toolResult();
    if (result === undefined) return;
    this.toolResultSent = true;
    this.post({ jsonrpc: "2.0", method: "ui/notifications/tool-result", params: result });
  }

  /** 主题变了 */
  notifyHostContext() {
    if (this.disposed || !this.toolInputSent) return;
    this.post({
      jsonrpc: "2.0",
      method: "ui/notifications/host-context-changed",
      params: this.host.hostContext(),
    });
  }

  get isInitialized() {
    return this.initialized;
  }
}

/** MCP Apps 规范的主题变量 → Multara 设计变量。iframe 跨源解析不了宿主变量，传计算后的值（R22） */
const STYLE_VARIABLES: Record<string, string> = {
  "--color-background-primary": "--color-bg",
  "--color-background-secondary": "--color-bg-secondary",
  "--color-background-tertiary": "--color-bg-tertiary",
  "--color-background-inverse": "--color-text",
  "--color-background-info": "--color-primary-subtle",
  "--color-background-danger": "--color-error-subtle",
  "--color-background-success": "--color-success-subtle",
  "--color-background-warning": "--color-warning-subtle",
  "--color-background-disabled": "--color-surface-hover",
  "--color-text-primary": "--color-text",
  "--color-text-secondary": "--color-text-secondary",
  "--color-text-tertiary": "--color-text-tertiary",
  "--color-text-inverse": "--color-bg",
  "--color-text-info": "--color-primary",
  "--color-text-danger": "--color-error",
  "--color-text-success": "--color-success",
  "--color-text-warning": "--color-warning",
  "--color-text-disabled": "--color-text-tertiary",
  "--color-text-ghost": "--color-text-secondary",
  "--color-border-primary": "--color-border",
  "--color-border-secondary": "--color-border-light",
  "--color-border-tertiary": "--color-border-light",
  "--color-border-inverse": "--color-text",
  "--color-border-info": "--color-primary-border",
  "--color-border-danger": "--color-error-border",
  "--color-border-success": "--color-success",
  "--color-border-warning": "--color-warning-border",
  "--color-border-disabled": "--color-text-tertiary",
  "--color-ring-primary": "--color-focus",
  "--color-ring-secondary": "--color-focus",
  "--color-ring-inverse": "--color-focus",
  "--color-ring-info": "--color-primary-border",
  "--color-ring-danger": "--color-error-border",
  "--color-ring-success": "--color-success",
  "--color-ring-warning": "--color-warning-border",
};

const FIXED_VARIABLES: Record<string, string> = {
  "--color-background-ghost": "transparent",
  "--color-border-ghost": "transparent",
  "--font-weight-normal": "normal",
  "--font-weight-medium": "500",
  "--font-weight-semibold": "600",
  "--font-weight-bold": "bold",
  "--border-radius-xs": "2px",
  "--border-radius-sm": "3px",
  "--border-radius-md": "4px",
  "--border-radius-lg": "6px",
  "--border-radius-xl": "8px",
  "--border-radius-full": "9999px",
};

export function hostStyleVariables(read: (name: string) => string): Record<string, string> {
  const variables: Record<string, string> = { ...FIXED_VARIABLES };
  for (const [name, source] of Object.entries(STYLE_VARIABLES)) {
    const value = read(source).trim();
    if (value) variables[name] = value;
  }
  const body = read("--font-family-base").trim() || read("--font-sans").trim();
  const mono = read("--font-family-mono").trim() || read("--font-mono").trim();
  if (body) variables["--font-sans"] = body;
  if (mono) variables["--font-mono"] = mono;
  return variables;
}
