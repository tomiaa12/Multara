// @vitest-environment node

import { describe, expect, it } from "vitest";
import type { Runtime } from "../domain/types";
import { filterRuntimes, isCustomRuntime, visibleRuntimes } from "./runtime-list";

function runtime(id: string, status: Runtime["status"]): Runtime {
  return {
    id,
    name: id,
    provider: "claude",
    command: "claude",
    args: [],
    path: null,
    version: null,
    status,
    maxConcurrent: 1,
  };
}

const runtimes = [
  runtime("claude", "online"),
  runtime("codex", "not_found"),
  runtime("custom:gateway", "online"),
  runtime("custom:missing", "not_found"),
];

describe("runtime list", () => {
  it("recognizes custom runtimes by their stable id prefix", () => {
    expect(isCustomRuntime(runtimes[2]!)).toBe(true);
    expect(isCustomRuntime(runtimes[0]!)).toBe(false);
  });

  it("keeps online built-ins and every custom runtime visible", () => {
    expect(visibleRuntimes(runtimes).map((item) => item.id)).toEqual([
      "claude",
      "custom:gateway",
      "custom:missing",
    ]);
  });

  it("filters visible runtimes without revealing missing built-ins", () => {
    expect(filterRuntimes(runtimes, "all")).toHaveLength(3);
    expect(filterRuntimes(runtimes, "builtin").map((item) => item.id)).toEqual(["claude"]);
    expect(filterRuntimes(runtimes, "custom").map((item) => item.id)).toEqual([
      "custom:gateway",
      "custom:missing",
    ]);
  });
});
