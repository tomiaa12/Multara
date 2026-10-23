// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  GATEWAY_FAMILIES,
  RUNTIME_FAMILIES,
  formatCommandLine,
  gatewayDraftFor,
  gatewayHost,
  headersTextFrom,
  parseCommandLine,
  parseHeadersText,
} from "./runtime-catalog";

describe("runtime catalog", () => {
  it("lets every catalog family be created", () => {
    expect(RUNTIME_FAMILIES.every((item) => item.supported)).toBe(true);
    expect(RUNTIME_FAMILIES.find((item) => item.id === "kimi")?.supported).toBe(true);
    expect(RUNTIME_FAMILIES.find((item) => item.id === "cursor")?.supported).toBe(true);
    expect(GATEWAY_FAMILIES.map((item) => item.id)).toEqual(["claude", "codex", "opencode", "gemini", "qwen"]);
  });
});

describe("parseCommandLine", () => {
  it("splits a pasted executable and fixed args", () => {
    expect(parseCommandLine("agent --model composer-2.5")).toEqual({
      ok: true,
      commandName: "agent",
      fixedArgs: ["--model", "composer-2.5"],
    });
  });

  it("preserves quoted whitespace and escaped characters", () => {
    expect(parseCommandLine(`agent --flag "a b c" path\\ with\\ spaces`)).toEqual({
      ok: true,
      commandName: "agent",
      fixedArgs: ["--flag", "a b c", "path with spaces"],
    });
  });

  it("rejects shell control syntax", () => {
    expect(parseCommandLine("agent && rm -rf /")).toEqual({ ok: false, error: "shell_syntax" });
    expect(parseCommandLine("agent | tee out")).toEqual({ ok: false, error: "shell_syntax" });
  });

  it("rejects shell expansion syntax", () => {
    expect(parseCommandLine("agent --path $HOME/bin")).toEqual({ ok: false, error: "shell_expansion" });
    expect(parseCommandLine("agent --path $(which foo)")).toEqual({ ok: false, error: "shell_expansion" });
  });

  it("allows literal shell-looking characters inside single quotes", () => {
    expect(parseCommandLine("agent --note '$5 reward `literal`'")).toEqual({
      ok: true,
      commandName: "agent",
      fixedArgs: ["--note", "$5 reward `literal`"],
    });
  });

  it("rejects unclosed quotes and a trailing escape", () => {
    expect(parseCommandLine(`agent --flag "unterminated`)).toEqual({ ok: false, error: "unclosed_quote" });
    expect(parseCommandLine("agent \\")).toEqual({ ok: false, error: "trailing_escape" });
  });
});

describe("formatCommandLine", () => {
  it("quotes args that need shell escaping for display", () => {
    expect(formatCommandLine("agent", ["--flag", "a b c"])).toBe('agent --flag "a b c"');
  });
});

describe("gatewayDraftFor", () => {
  it("drops the previous CLI gateway fields when the CLI changes", () => {
    const next = gatewayDraftFor("codex", {
      name: "公司网关",
      apiUrl: "https://claude.example",
      apiKey: "sk-old",
      modelsText: "sonnet",
      commandLine: "claude --model opus",
      maxConcurrent: 4,
    });
    expect(next.provider).toBe("codex");
    expect(next.name).toBe("公司网关");
    expect(next.commandLine).toBe("codex");
    expect(next.apiUrl).toBe("");
    expect(next.apiKey).toBe("");
    expect(next.modelsText).toBe("");
    expect(next.apiFormat).toBe("responses");
    expect(next.maxConcurrent).toBe(4);
  });

  it("uses each CLI native API format for new gateways", () => {
    expect(gatewayDraftFor("claude").apiFormat).toBe("anthropic");
    expect(gatewayDraftFor("gemini").apiFormat).toBe("gemini");
    expect(gatewayDraftFor("qwen").apiFormat).toBe("chat");
    expect(gatewayDraftFor("opencode").apiFormat).toBe("chat");
  });
});

describe("gatewayHost", () => {
  it("returns the hostname of a base URL", () => {
    expect(gatewayHost("https://aigw.example.com/v1/")).toBe("aigw.example.com");
  });
});

describe("gateway headers", () => {
  it("round trips custom request headers and ignores malformed lines", () => {
    const headers = parseHeadersText("X-Trace: abc\ninvalid\nX-Mode: fast");
    expect(headers).toEqual({ "X-Trace": "abc", "X-Mode": "fast" });
    expect(headersTextFrom(headers)).toContain("X-Trace: abc");
  });
});
