// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { ChatMessage, TimelineItem } from "../../domain/types";
import { inboxBrowserLinks, normalizeBrowserUrl } from "./inbox-browser-links";

function item(partial: Partial<TimelineItem>): TimelineItem {
  return {
    seq: 1,
    eventType: "tool_use",
    callId: null,
    tool: null,
    content: null,
    input: null,
    output: null,
    createdAt: "",
    ...partial,
  };
}

function message(timeline: TimelineItem[]): ChatMessage {
  return {
    id: "m",
    sessionId: "s",
    role: "assistant",
    content: "see https://ignored.example",
    runId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline,
    attachments: [],
    createdAt: "",
  };
}

describe("inbox browser links", () => {
  it("collects http urls from tool input and output, in order, without search text", () => {
    const links = inboxBrowserLinks([
      message([
        item({ tool: "WebSearch", input: { query: "vue docs" } }),
        item({ tool: "WebFetch", input: { url: "https://vuejs.org/guide/" } }),
        item({ tool: "browser", output: "opened https://vuejs.org/guide/ and https://example.com/a." }),
      ]),
    ]);
    expect(links).toEqual(["https://vuejs.org/guide/", "https://example.com/a"]);
  });

  it("returns nothing when the transcript has no timeline urls", () => {
    expect(inboxBrowserLinks(undefined)).toEqual([]);
    expect(inboxBrowserLinks([message([])])).toEqual([]);
  });

  it("adds https when the address has no scheme", () => {
    expect(normalizeBrowserUrl(" example.com/a ")).toBe("https://example.com/a");
    expect(normalizeBrowserUrl("http://example.com")).toBe("http://example.com");
    expect(normalizeBrowserUrl("  ")).toBe("");
  });
});
