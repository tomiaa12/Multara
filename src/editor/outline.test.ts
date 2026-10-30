// @vitest-environment node
import { describe, expect, test } from "vitest";
import { OUTLINE_MAX_CHARS, OUTLINE_MAX_LINES, enclosingOutline, fileBreadcrumbs, outlineFromContent } from "./outline";

describe("outline", () => {
  test("reads markdown headings and the enclosing symbol", () => {
    const items = outlineFromContent("markdown", "# A\n\n## B\ntext\n");
    expect(items.map((item) => item.label)).toEqual(["A", "B"]);
    expect(enclosingOutline(items, 4)?.label).toBe("B");
    expect(enclosingOutline(items, 1)?.label).toBe("A");
  });

  test("reads rust functions and file breadcrumbs", () => {
    expect(outlineFromContent("rust", "pub fn save() {}\n").map((item) => item.label)).toEqual([
      "save",
    ]);
    expect(fileBreadcrumbs("src/editor/eol.ts")).toEqual(["src", "editor", "eol.ts"]);
  });

  test("drops a heading once the cursor leaves that element", () => {
    const content = [
      "<h1>{{ $t('CloseAccount.title') }}</h1>",
      "<script>",
      "export function closeAccount() {",
      "  return true",
      "}",
      "</script>",
    ].join("\n");
    const items = outlineFromContent("html", content);
    expect(enclosingOutline(items, 1)?.label).toContain("CloseAccount.title");
    expect(enclosingOutline(items, 4)).toBeNull();
  });

  test("computes end lines for code symbols and nested headings", () => {
    const code = ["function a() {", "  return 1;", "}", "", "function b() {", "}"].join("\n");
    expect(outlineFromContent("typescript", code).map((item) => [item.label, item.line, item.endLine])).toEqual([
      ["a", 1, 3],
      ["b", 5, 6],
    ]);
    const markdown = "# A\n## B\ntext\n# C\nmore";
    expect(outlineFromContent("markdown", markdown).map((item) => [item.label, item.endLine])).toEqual([
      ["A", 3],
      ["B", 3],
      ["C", 5],
    ]);
  });

  test("skips oversized files", () => {
    const manyLines = "# A\n".repeat(OUTLINE_MAX_LINES + 1);
    expect(outlineFromContent("markdown", manyLines)).toEqual([]);
    expect(outlineFromContent("markdown", `# A\n${"x".repeat(OUTLINE_MAX_CHARS)}`)).toEqual([]);
  });
});
