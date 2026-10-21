import { describe, expect, it } from "vitest";
import { activeMentionQuery, filterCommands, replaceMentionToken } from "./composer-mentions";

describe("composer mentions", () => {
  it("reads trailing @ and # queries", () => {
    expect(activeMentionQuery("请 @mi")).toEqual({ kind: "@", query: "mi" });
    expect(activeMentionQuery("看 #登")).toEqual({ kind: "#", query: "登" });
    expect(activeMentionQuery("@miku 继续")).toBeNull();
  });

  it("replaces the mention token", () => {
    expect(replaceMentionToken([{ type: "text", value: "请 @mi" }], "@Miku ")).toEqual([
      { type: "text", value: "请 @Miku " },
    ]);
  });

  it("lists compact and fork commands", () => {
    expect(filterCommands("comp").map((item) => item.label)).toEqual(["compact"]);
  });
});
