import { describe, expect, it } from "vitest";
import { elapsedMs, formatDuration, nodeStatusTone, summarizeInputs, summarizeValue } from "./run-format";

describe("run-format", () => {
  it("formats durations", () => {
    expect(formatDuration(450)).toBe("450ms");
    expect(formatDuration(12_000)).toBe("12s");
    expect(formatDuration(184_000)).toBe("3m 04s");
    expect(formatDuration(3_900_000)).toBe("1h 5m");
    expect(formatDuration(null)).toBe("—");
  });
  it("computes elapsed with now fallback", () => {
    expect(elapsedMs("2026-01-01T00:00:00Z", "2026-01-01T00:00:05Z")).toBe(5000);
    expect(elapsedMs("2026-01-01T00:00:00Z", null, Date.parse("2026-01-01T00:00:03Z"))).toBe(3000);
    expect(elapsedMs(null, null)).toBeNull();
  });
  it("summarizes values", () => {
    expect(summarizeValue({ a: 1 })).toBe('{"a":1}');
    expect(summarizeValue("x".repeat(100), 10)).toHaveLength(10);
    expect(summarizeInputs({ a: "b", c: 2 })).toBe("a=b, c=2");
  });
  it("maps tones", () => {
    expect(nodeStatusTone("waiting_human")).toBe("warning");
    expect(nodeStatusTone("pending")).toBeUndefined();
  });
});
