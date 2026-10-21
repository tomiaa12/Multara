import { describe, expect, it } from "vitest";
import { detectedNeedsAuth, jsonToMcpInputs, mcpInputToJson } from "./mcp-json";
import type { McpInput } from "../domain/types";

const stdio: McpInput = {
  name: "filesystem",
  previousName: "",
  owner: "multara",
  transport: "stdio",
  command: "npx",
  cwd: "",
  envFile: "",
  args: ["-y", "@modelcontextprotocol/server-filesystem"],
  url: "",
  envText: "API_KEY=demo",
  headersText: "",
  needsAuth: false,
  enabled: true,
};

describe("mcp-json", () => {
  it("writes one server config without the mcpServers wrapper", () => {
    const json = mcpInputToJson(stdio, stdio.args.join("\n"));
    const parsed = JSON.parse(json) as Record<string, unknown>;
    expect(parsed.mcpServers).toBeUndefined();
    expect(parsed.command).toBe("npx");
    const [input] = jsonToMcpInputs(json);
    expect(input.name).toBe("");
    expect(input.command).toBe("npx");
    expect(input.args).toEqual(stdio.args);
    expect(input.envText).toContain("API_KEY=demo");
  });

  it("keeps oauth on the single http config", () => {
    const json = mcpInputToJson({
      ...stdio,
      transport: "http",
      command: "",
      args: [],
      url: "https://project.feishu.cn/mcp_server/v1",
      envText: "",
      needsAuth: true,
    }, "");
    const parsed = JSON.parse(json) as Record<string, unknown>;
    expect(parsed).toEqual({
      type: "http",
      url: "https://project.feishu.cn/mcp_server/v1",
      oauth: {},
    });
    expect(jsonToMcpInputs(json)[0].needsAuth).toBe(true);
  });

  it("parses a pasted mcpServers document with http", () => {
    const [parsed] = jsonToMcpInputs(`{
      "mcpServers": {
        "docs": {
          "type": "http",
          "url": "https://example.com/mcp",
          "headers": { "Authorization": "Bearer x" }
        }
      }
    }`);
    expect(parsed.transport).toBe("http");
    expect(parsed.url).toBe("https://example.com/mcp");
    expect(parsed.headersText).toContain("Authorization: Bearer x");
  });

  it("writes cwd, envFile and JSON args", () => {
    const json = mcpInputToJson({
      ...stdio,
      cwd: "${workspaceFolder}",
      envFile: "${workspaceFolder}/.env",
      args: ["-y", "my server"],
    }, JSON.stringify(["-y", "my server"]));
    const parsed = JSON.parse(json) as Record<string, unknown>;
    expect(parsed.cwd).toBe("${workspaceFolder}");
    expect(parsed.envFile).toBe("${workspaceFolder}/.env");
    expect(parsed.args).toEqual(["-y", "my server"]);
  });

  it("parses sse remotes", () => {
    const [parsed] = jsonToMcpInputs(`{ "type": "sse", "url": "https://example.com/sse" }`);
    expect(parsed.transport).toBe("sse");
    expect(parsed.url).toBe("https://example.com/sse");
  });

  it("treats a remote HTTP server without a real token as needing auth", () => {
    expect(detectedNeedsAuth({ transport: "http", headersText: "", envText: "" })).toBe(true);
    expect(detectedNeedsAuth({
      transport: "http",
      headersText: "Authorization: Bearer ${input:token}",
      envText: "",
    })).toBe(true);
    expect(detectedNeedsAuth({
      transport: "http",
      headersText: "Authorization: Bearer static-token",
      envText: "",
    })).toBe(false);
  });
});
