import { describe, expect, it } from "vitest";
import type { TimelineItem } from "../../../domain/types";
import { toolResultUis } from "./tool-ui";

let seq = 0;
function event(partial: Partial<TimelineItem> & Pick<TimelineItem, "eventType">): TimelineItem {
  seq += 1;
  return { seq, callId: null, tool: null, content: null, input: null, output: null, createdAt: "", ...partial };
}

const ui = { type: "table", props: { columns: ["title"], rows: [{ title: "x" }] } };

function pair(command: string, output: string, callId = `c${seq}`) {
  return [
    event({ eventType: "tool_use", tool: "Bash", callId, input: { command } }),
    event({ eventType: "tool_result", tool: "Bash", callId, output }),
  ];
}

describe("toolResultUis", () => {
  it("lifts _ui from multara commands", () => {
    const items = toolResultUis(pair("multara task list --ui", JSON.stringify({ items: [], _ui: ui })));
    expect(items).toHaveLength(1);
    expect(items[0].spec.type).toBe("table");
    expect(items[0].command).toBe("multara task list --ui");
  });

  it("accepts the absolute CLI path", () => {
    expect(toolResultUis(pair('"$MULTARA_CLI" task list --ui', JSON.stringify({ _ui: ui })))).toHaveLength(1);
    expect(toolResultUis(pair("/x/bin/multara task list --ui", JSON.stringify({ _ui: ui })))).toHaveLength(1);
  });

  it("ignores other commands, plain output and question blocks", () => {
    expect(toolResultUis(pair("cat out.json", JSON.stringify({ _ui: ui })))).toEqual([]);
    expect(toolResultUis(pair("multara task list", "[]"))).toEqual([]);
    expect(toolResultUis(pair("multara task list --ui", "{oops _ui"))).toEqual([]);
    expect(toolResultUis(pair("multara x --ui", JSON.stringify({ _ui: { type: "question", props: {} } })))).toEqual([]);
  });
});
