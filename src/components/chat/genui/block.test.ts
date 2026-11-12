// @vitest-environment node
import { describe, expect, test } from "vitest";
import { isCanvasEnabled, isGenUiEnabled, isMcpAppsEnabled } from "./block";

describe("experimental feature defaults", () => {
  test("stays off until settings have loaded", () => {
    expect(isGenUiEnabled(null)).toBe(false);
    expect(isGenUiEnabled(undefined)).toBe(false);
    expect(isCanvasEnabled(null)).toBe(false);
    expect(isMcpAppsEnabled(null)).toBe(false);
  });

  test("turns on when the key was never written", () => {
    expect(isGenUiEnabled({})).toBe(true);
    expect(isCanvasEnabled({})).toBe(true);
    expect(isMcpAppsEnabled({})).toBe(true);
  });

  test("turns off only for an explicit false", () => {
    expect(isGenUiEnabled({ genuiEnabled: "false" })).toBe(false);
    expect(isCanvasEnabled({ genuiEnabled: "true", canvasesEnabled: "false" })).toBe(false);
    expect(isMcpAppsEnabled({ genuiEnabled: "true", mcpAppsEnabled: "false" })).toBe(false);
  });

  test("keeps canvas and mcp apps behind generative UI", () => {
    expect(isCanvasEnabled({ genuiEnabled: "false", canvasesEnabled: "true" })).toBe(false);
    expect(isMcpAppsEnabled({ genuiEnabled: "false", mcpAppsEnabled: "true" })).toBe(false);
  });
});
