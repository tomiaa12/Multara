// @vitest-environment node
import { describe, expect, test } from "vitest";
import type { TimelineItem } from "../../domain/types";
import { previewFromTimelineItem } from "./agent-preview";

function item(partial: Partial<TimelineItem> & Pick<TimelineItem, "eventType">): TimelineItem {
  return {
    seq: 1,
    tool: "",
    callId: null,
    parentCallId: null,
    content: null,
    input: null,
    output: null,
    replacesFinalText: false,
    createdAt: "",
    ...partial,
  };
}

describe("previewFromTimelineItem", () => {
  test("does not open empty terminal for shell tools", () => {
    expect(
      previewFromTimelineItem(
        item({
          eventType: "tool_use",
          tool: "Shell",
          input: { command: "npm test" },
        }),
      ),
    ).toBeNull();
  });

  test("opens browser for navigate tools with http url", () => {
    expect(
      previewFromTimelineItem(
        item({
          eventType: "tool_use",
          tool: "mcp_chrome-browser-plugin_browser_navigate",
          input: { url: "https://example.com/path" },
        }),
      ),
    ).toEqual({ type: "browser", url: "https://example.com/path" });
  });

  test("opens file after write/edit tool_result", () => {
    expect(
      previewFromTimelineItem(
        item({
          eventType: "tool_result",
          tool: "Write",
          input: { path: "src/App.vue", contents: "x" },
        }),
      ),
    ).toEqual({ type: "file", path: "src/App.vue" });
  });

  test("ignores read tools", () => {
    expect(
      previewFromTimelineItem(
        item({
          eventType: "tool_result",
          tool: "Read",
          input: { path: "src/App.vue" },
        }),
      ),
    ).toBeNull();
  });
});
