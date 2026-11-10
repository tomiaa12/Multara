import { describe, expect, it, vi } from "vitest";
import { McpAppBridge, contentText, hostStyleVariables, type McpAppHost } from "./McpAppBridge";

function setup(overrides: Partial<McpAppHost> = {}) {
  const posted: Record<string, unknown>[] = [];
  let result: unknown;
  const host: McpAppHost = {
    hostInfo: { name: "Multara", version: "1.0" },
    csp: null,
    clipboardWrite: false,
    toolInput: () => ({ city: "Paris" }),
    toolResult: () => result,
    hostContext: () => ({ theme: "dark" }),
    call: vi.fn(async () => ({ content: [{ type: "text", text: "ok" }] })),
    fillComposer: vi.fn(() => ({ ok: true })),
    attachContext: vi.fn(),
    openLink: vi.fn(async () => true),
    setHeight: vi.fn(),
    maxHeight: () => 600,
    ...overrides,
  };
  const bridge = new McpAppBridge((message) => posted.push(message as Record<string, unknown>), host);
  return { bridge, host, posted, setResult: (value: unknown) => (result = value) };
}

const methods = (posted: Record<string, unknown>[]) => posted.map((item) => item.method ?? `#${String(item.id)}`);

describe("McpAppBridge", () => {
  it("pushes tool-input only after the initialize reply, then the result when it arrives", async () => {
    const { bridge, posted, setResult } = setup();
    await bridge.handle({ jsonrpc: "2.0", id: 1, method: "ui/initialize", params: {} });
    expect(methods(posted)).toEqual(["#1", "ui/notifications/tool-input"]);
    const reply = posted[0].result as Record<string, unknown>;
    expect(reply.protocolVersion).toBe("2026-01-26");
    expect(reply.hostContext).toEqual({ theme: "dark" });
    expect(posted[1].params).toEqual({ arguments: { city: "Paris" } });
    // 工具还没完成：不推结果
    bridge.notifyToolResult();
    expect(posted).toHaveLength(2);
    setResult({ content: [], structuredContent: { t: 21 } });
    bridge.notifyToolResult();
    bridge.notifyToolResult();
    expect(methods(posted)).toEqual(["#1", "ui/notifications/tool-input", "ui/notifications/tool-result"]);
    expect(posted[2].params).toEqual({ content: [], structuredContent: { t: 21 } });
  });

  it("sends the result right after initialize when the tool already finished", async () => {
    const { bridge, posted, setResult } = setup();
    setResult({ content: [] });
    await bridge.handle({ jsonrpc: "2.0", id: "a", method: "ui/initialize" });
    expect(methods(posted)).toEqual(["#a", "ui/notifications/tool-input", "ui/notifications/tool-result"]);
    // 再次 initialize（页面重载）：重新推一遍
    await bridge.handle({ jsonrpc: "2.0", id: "b", method: "ui/initialize" });
    expect(methods(posted).slice(3)).toEqual(["#b", "ui/notifications/tool-input", "ui/notifications/tool-result"]);
  });

  it("forwards tools/call and resources/read to the server", async () => {
    const { bridge, host, posted } = setup();
    await bridge.handle({ jsonrpc: "2.0", id: 7, method: "tools/call", params: { name: "refresh", arguments: {} } });
    expect(host.call).toHaveBeenCalledWith("tools/call", { name: "refresh", arguments: {} });
    expect(posted[0]).toMatchObject({ id: 7, result: { content: [{ text: "ok" }] } });
  });

  it("reports server errors back to the page", async () => {
    const { bridge, posted } = setup({ call: vi.fn(async () => { throw new Error("Tool not found: x"); }) });
    await bridge.handle({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "x" } });
    expect(posted[0]).toMatchObject({ id: 2, error: { message: "Tool not found: x" } });
  });

  it("rejects unknown methods with -32601", async () => {
    const { bridge, posted } = setup();
    await bridge.handle({ jsonrpc: "2.0", id: 3, method: "sampling/createMessage", params: {} });
    expect(posted[0]).toMatchObject({ id: 3, error: { code: -32601 } });
  });

  it("only fills the composer and attaches context; never sends", async () => {
    const fill = vi.fn(() => ({ ok: false }));
    const { bridge, host, posted } = setup({ fillComposer: fill });
    await bridge.handle({ jsonrpc: "2.0", id: 4, method: "ui/message", params: { role: "user", content: [{ type: "text", text: "hi" }] } });
    expect(fill).toHaveBeenCalledWith("hi");
    expect(posted[0]).toMatchObject({ id: 4, result: { isError: true } });
    await bridge.handle({
      jsonrpc: "2.0",
      id: 5,
      method: "ui/update-model-context",
      params: { content: [{ type: "text", text: "picked Paris" }], structuredContent: { city: "Paris" } },
    });
    expect(host.attachContext).toHaveBeenCalledWith("picked Paris", { city: "Paris" });
  });

  it("only opens http(s) links and keeps display mode inline", async () => {
    const { bridge, host, posted } = setup();
    await bridge.handle({ jsonrpc: "2.0", id: 6, method: "ui/open-link", params: { url: "javascript:alert(1)" } });
    expect(host.openLink).not.toHaveBeenCalled();
    expect(posted[0]).toMatchObject({ result: { isError: true } });
    await bridge.handle({ jsonrpc: "2.0", id: 8, method: "ui/open-link", params: { url: "https://example.com" } });
    expect(host.openLink).toHaveBeenCalledWith("https://example.com");
    await bridge.handle({ jsonrpc: "2.0", id: 9, method: "ui/request-display-mode", params: { mode: "fullscreen" } });
    expect(posted.at(-1)).toMatchObject({ id: 9, result: { mode: "inline" } });
  });

  it("clamps size changes and drops cancelled replies", async () => {
    let release: (value: unknown) => void = () => undefined;
    const { bridge, host, posted } = setup({ call: vi.fn(() => new Promise((resolve) => { release = resolve; })) });
    await bridge.handle({ jsonrpc: "2.0", method: "ui/notifications/size-changed", params: { height: 5 } });
    await bridge.handle({ jsonrpc: "2.0", method: "ui/notifications/size-changed", params: { height: 9999 } });
    expect(host.setHeight).toHaveBeenNthCalledWith(1, 38);
    expect(host.setHeight).toHaveBeenNthCalledWith(2, 600);
    const pending = bridge.handle({ jsonrpc: "2.0", id: 10, method: "tools/call", params: { name: "slow" } });
    await bridge.handle({ jsonrpc: "2.0", method: "notifications/cancelled", params: { requestId: 10 } });
    release({ content: [] });
    await pending;
    expect(posted.find((item) => item.id === 10)).toBeUndefined();
  });

  it("pushes host context changes after initialize only", async () => {
    const { bridge, posted } = setup();
    bridge.notifyHostContext();
    expect(posted).toHaveLength(0);
    await bridge.handle({ jsonrpc: "2.0", id: 1, method: "ui/initialize" });
    bridge.notifyHostContext();
    expect(posted.at(-1)?.method).toBe("ui/notifications/host-context-changed");
  });

  it("ignores everything after dispose", async () => {
    const { bridge, posted } = setup();
    bridge.dispose();
    await bridge.handle({ jsonrpc: "2.0", id: 1, method: "ping" });
    expect(posted).toHaveLength(0);
  });
});

describe("helpers", () => {
  it("joins text content blocks", () => {
    expect(contentText([{ type: "text", text: "a" }, { type: "image", data: "x" }, { type: "text", text: "b" }])).toBe("a\nb");
    expect(contentText("nope")).toBe("");
  });

  it("maps host colors to computed values", () => {
    const values: Record<string, string> = { "--color-bg": " #111 ", "--color-text": "#eee" };
    const vars = hostStyleVariables((name) => values[name] ?? "");
    expect(vars["--color-background-primary"]).toBe("#111");
    expect(vars["--color-text-primary"]).toBe("#eee");
    expect(vars["--color-background-ghost"]).toBe("transparent");
    expect(vars["--color-text-danger"]).toBeUndefined();
  });
});
