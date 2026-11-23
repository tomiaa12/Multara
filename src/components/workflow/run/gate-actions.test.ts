import { describe, expect, test } from "vitest";
import { gateActionsFromText, stripGateActions } from "./gate-actions";

describe("gate actions", () => {
  test("reads the last multara-actions block", () => {
    const text = [
      "推荐 pages。",
      "```multara-actions",
      JSON.stringify({ actions: [{ id: "old", label: "旧的" }] }),
      "```",
      "改成这三个仓。",
      "```multara-actions",
      JSON.stringify({
        actions: [
          { id: "approve", label: "用 pages、uc 建仓" },
          { id: "reject", label: "" },
        ],
      }),
      "```",
    ].join("\n");
    expect(gateActionsFromText(text)).toEqual([{ id: "approve", label: "用 pages、uc 建仓" }]);
    expect(stripGateActions(text)).toBe("推荐 pages。\n改成这三个仓。");
  });
});