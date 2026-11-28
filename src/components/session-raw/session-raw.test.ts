import { describe, expect, it } from "vitest";
import type { RawRunEvent } from "../../services/backend";
import {
  appendEvents,
  formatCommandLine,
  previewText,
  searchableEventText,
  shellQuote,
} from "./session-raw";

function event(seq: number, content: string | null = null): RawRunEvent {
  return {
    seq,
    eventType: "tool_result",
    callId: null,
    tool: "Read",
    content,
    inputJson: null,
    output: null,
    outputJson: null,
    createdAt: "now",
    parentCallId: null,
    status: "completed",
  };
}

describe("session raw helpers", () => {
  it("quotes only arguments that need it", () => {
    expect(shellQuote("--model")).toBe("--model");
    expect(shellQuote("a b")).toBe("'a b'");
    expect(shellQuote("it's")).toBe("'it'\\''s'");
    expect(formatCommandLine("claude", ["-p", "hello world"])).toBe("claude -p 'hello world'");
  });

  it("merges event pages by seq without duplicates", () => {
    const merged = appendEvents([event(2), event(1)], [event(2, "updated"), event(3)]);
    expect(merged.map((item) => item.seq)).toEqual([1, 2, 3]);
    expect(merged[1]?.content).toBe("updated");
  });

  it("previews long text and reports clipping", () => {
    expect(previewText("short", 10)).toEqual({ text: "short", clipped: false });
    expect(previewText("0123456789abc", 10)).toEqual({ text: "0123456789", clipped: true });
  });

  it("searches across event fields case-insensitively", () => {
    expect(searchableEventText(event(1, "Hello WORLD"))).toContain("hello world");
    expect(searchableEventText(event(1))).toContain("read");
  });
});
