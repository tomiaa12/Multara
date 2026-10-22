import { describe, expect, it } from "vitest";
import { mcpProbeErrorKey } from "./mcp-probe-error";

describe("mcpProbeErrorKey", () => {
  it("maps npx missing", () => {
    expect(mcpProbeErrorKey("无法启动 npx：No such file or directory (os error 2)")).toBe("mcp.error.npxMissing");
    expect(mcpProbeErrorKey("找不到命令 npx。请确认已安装")).toBe("mcp.error.npxMissing");
  });

  it("maps generic command missing", () => {
    expect(mcpProbeErrorKey("找不到命令 uvx。")).toBe("mcp.error.commandMissing");
  });

  it("maps auth and timeout", () => {
    expect(mcpProbeErrorKey("服务器要求授权（HTTP 401）")).toBe("mcp.error.auth");
    expect(mcpProbeErrorKey("等待服务器响应超时")).toBe("mcp.error.timeout");
  });

  it("returns null for unknown messages", () => {
    expect(mcpProbeErrorKey("weird upstream failure xyz")).toBeNull();
  });
});
