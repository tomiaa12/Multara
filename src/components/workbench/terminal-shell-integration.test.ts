// @vitest-environment node
import { describe, expect, it } from "vitest";
import { joinInputRows, parseOsc633, parseOsc7 } from "./terminal-shell-integration";

describe("parseOsc633", () => {
  it("reads prompt and command boundaries", () => {
    expect(parseOsc633("A")).toEqual({ kind: "promptStart" });
    expect(parseOsc633("B")).toEqual({ kind: "commandStart" });
    expect(parseOsc633("C")).toEqual({ kind: "executed" });
  });

  it("reads the exit code only when there is one", () => {
    expect(parseOsc633("D;0")).toEqual({ kind: "finished", exitCode: 0 });
    expect(parseOsc633("D;127")).toEqual({ kind: "finished", exitCode: 127 });
    expect(parseOsc633("D")).toEqual({ kind: "finished" });
  });

  it("keeps semicolons and equals signs inside the cwd", () => {
    expect(parseOsc633("P;Cwd=C:\\a;b=c")).toEqual({ kind: "cwd", cwd: "C:\\a;b=c" });
    expect(parseOsc633("P;Other=1")).toBeNull();
  });
});

describe("parseOsc7", () => {
  it("decodes unix and windows file urls", () => {
    expect(parseOsc7("file://mac/Users/me/My%20Code")).toBe("/Users/me/My Code");
    expect(parseOsc7("file://pc/C:/work")).toBe("C:/work");
    expect(parseOsc7("http://x/y")).toBeNull();
  });
});

describe("joinInputRows", () => {
  it("joins wrapped rows and keeps real line breaks", () => {
    expect(joinInputRows([
      { text: "PS C:\\> git commit -m 'a", wrapped: false },
      { text: "bc'", wrapped: true },
    ], 7)).toBe("git commit -m 'abc'");
    expect(joinInputRows([
      { text: "$ echo a \\", wrapped: false },
      { text: "> b", wrapped: false },
    ], 2)).toBe("echo a \\\n> b");
  });
});
