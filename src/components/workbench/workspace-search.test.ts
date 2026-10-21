import { describe, expect, test } from "vitest";
import { emptySearchOptions, globMatch, replaceInDocument, searchDocument } from "./workspace-search";

describe("workspace search", () => {
  test("matches a line and respects include globs", () => {
    const options = { ...emptySearchOptions(), query: "Title", include: "**/*.vue" };
    const found = searchDocument("src/App.vue", "const name = \"Title\"\n", options);
    expect(found.matches).toHaveLength(1);
    expect(found.matches[0].line).toBe(1);
    expect(found.matches[0].preview).toContain("Title");
    expect(searchDocument("src/app.ts", "Title\n", options).matches).toHaveLength(0);
    expect(searchDocument("node_modules/pkg/a.vue", "Title\n", options).matches).toHaveLength(0);
  });

  test("supports whole word, case, and replace", () => {
    const options = { ...emptySearchOptions(), query: "cat", wholeWord: true, caseSensitive: true };
    const found = searchDocument("a.txt", "cat catalog CAT\n", options);
    expect(found.matches).toHaveLength(1);
    expect(found.matches[0].column).toBe(1);
    expect(replaceInDocument("cat catalog", options, "dog")).toBe("dog catalog");
  });

  test("globs match basenames and nested folders", () => {
    expect(globMatch("src/App.vue", "*.vue")).toBe(true);
    expect(globMatch("src/App.vue", "**/*.vue")).toBe(true);
    expect(globMatch("src/App.vue", "*.ts")).toBe(false);
  });

  test("a folder path includes files inside it", () => {
    expect(globMatch("src/components/App.vue", "./src/components")).toBe(true);
    expect(globMatch("src/components/nested/a.ts", "src/components")).toBe(true);
    expect(globMatch("src/components-extra/a.ts", "./src/components")).toBe(false);
    expect(globMatch("src/App.vue", "src/App.vue")).toBe(true);
  });
});
