// @vitest-environment node
import { describe, expect, test } from "vitest";
import { convertEol, detectEol, detectIndent, indentLabel } from "./eol";

describe("eol", () => {
  test("detects and converts line endings", () => {
    expect(detectEol("a\r\nb")).toBe("CRLF");
    expect(detectEol("a\nb")).toBe("LF");
    expect(convertEol("a\r\nb\nc", "LF")).toBe("a\nb\nc");
    expect(convertEol("a\nb", "CRLF")).toBe("a\r\nb");
  });

  test("detects indent from file content", () => {
    expect(detectIndent("\tfoo\n\tbar")).toEqual({ insertSpaces: false, tabSize: 4 });
    expect(detectIndent("  foo\n  bar")).toEqual({ insertSpaces: true, tabSize: 2 });
    expect(indentLabel(true, 2)).toBe("空格: 2");
    expect(indentLabel(false, 4)).toBe("Tab 大小: 4");
  });
});
