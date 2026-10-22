import { beforeAll, describe, expect, it } from "vitest";
import type { McpProbe } from "../domain/types";
import { applyLanguageSetting } from "../i18n";
import { mcpToolsCaption } from "./mcp-tools-caption";

beforeAll(() => applyLanguageSetting("zh-CN"));

function probe(partial: Partial<McpProbe>): McpProbe {
  return {
    ok: true,
    status: "running",
    serverName: "",
    serverVersion: "",
    protocolVersion: "",
    instructions: "",
    tools: [],
    resources: [],
    prompts: [],
    log: "",
    error: "",
    durationMs: 0,
    missingInputs: [],
    ...partial,
  };
}

describe("mcpToolsCaption", () => {
  it("shows count after a successful probe", () => {
    expect(mcpToolsCaption({
      probe: probe({ tools: [{ name: "a", title: "", description: "", uri: "" }, { name: "b", title: "", description: "", uri: "" }] }),
    })).toBe("2 个工具");
  });

  it("guides the user before probing", () => {
    expect(mcpToolsCaption({})).toBe("查看工具");
    expect(mcpToolsCaption({ loading: true })).toBe("加载中…");
  });

  it("shows disabled instead of tools when off", () => {
    expect(mcpToolsCaption({
      enabled: false,
      probe: probe({ tools: [{ name: "a", title: "", description: "", uri: "" }] }),
    })).toBe("已停用");
  });

  it("shows needs auth before treating as failed", () => {
    expect(mcpToolsCaption({
      probe: probe({ status: "needs_auth", ok: false, error: "HTTP 401", tools: [] }),
    })).toBe("需要授权");
  });
});
