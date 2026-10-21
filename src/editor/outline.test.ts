// @vitest-environment node
import { describe, expect, test } from "vitest";
import { enclosingOutline, fileBreadcrumbs, outlineFromContent } from "./outline";

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
});
