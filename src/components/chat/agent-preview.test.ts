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

  test("does not open editor after write/edit tool_result", () => {
    expect(
      previewFromTimelineItem(
        item({
          eventType: "tool_result",
          tool: "Write",
          input: { path: "src/App.vue", contents: "x" },
        }),
      ),
    ).toBeNull();
    expect(
      previewFromTimelineItem(
        item({
          eventType: "tool_result",
          tool: "Edit",
          input: { path: "src/App.vue", old_string: "a", new_string: "b" },
        }),
      ),
    ).toBeNull();
  });

  test("opens file for intentional Open/Reveal tools", () => {
    expect(
      previewFromTimelineItem(
        item({
          eventType: "tool_result",
          tool: "OpenFile",
          input: { path: "src/App.vue", start_line: 12 },
        }),
      ),
    ).toEqual({ type: "file", path: "src/App.vue", startLine: 12, endLine: 12 });
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
