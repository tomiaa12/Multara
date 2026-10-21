import { describe, expect, it } from "vitest";
import { mcpBrandFallbackColor, mcpBrandIconUrl, mcpBrandInitial } from "./mcp-brand-icon";

describe("mcpBrandIconUrl", () => {
  it("matches Feishu / Lark by name or url", () => {
    expect(mcpBrandIconUrl({ name: "feishu-project", summary: "" })).toContain("feishu.cn");
    expect(mcpBrandIconUrl({
      name: "project",
      summary: "https://project.feishu.cn/mcp_server/v1",
    })).toContain("feishu.cn");
  });

  it("falls back to host favicon for other http MCP", () => {
    expect(mcpBrandIconUrl({
      name: "custom",
      summary: "https://mcp.example.com/v1",
    })).toBe("https://mcp.example.com/favicon.ico");
  });

  it("returns undefined when there is no brand or url", () => {
    expect(mcpBrandIconUrl({ name: "local-fs", summary: "npx @modelcontextprotocol/server-filesystem" })).toBeUndefined();
  });
});

describe("mcpBrand fallback", () => {
  it("uses first character and stable color", () => {
    expect(mcpBrandInitial("chrome-devtools")).toBe("C");
    expect(mcpBrandFallbackColor("chrome-devtools")).toBe(mcpBrandFallbackColor("chrome-devtools"));
    expect(mcpBrandFallbackColor("a")).not.toBe(mcpBrandFallbackColor("b"));
  });
});
