// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  componentFileNames,
  definitionColumn,
  findDefinitionLines,
  normalizeDriveLetter,
  resolveImportSpecifier,
  searchIncludeFor,
  stringLiteralAt,
} from "./definition-heuristics";

describe("definition heuristics", () => {
  test("finds definitions across languages and points at the name", () => {
    const lines = [
      "pub fn start_server() {}",
      "  def start_server(self):",
      "func (s *Server) start_server() {}",
      "start_server();",
      "export const start_server = () => {};",
    ];
    expect(findDefinitionLines(lines, "start_server")).toEqual([
      { line: 1, column: 8 },
      { line: 2, column: 7 },
      { line: 3, column: 18 },
      { line: 5, column: 14 },
    ]);
  });

  test("escapes identifiers with regex characters", () => {
    expect(findDefinitionLines(["const $store = 1", "const xstore = 2"], "$store")).toEqual([
      { line: 1, column: 7 },
    ]);
  });

  test("converts a search match into the name column", () => {
    expect(definitionColumn(5, "class Foo".length, "Foo")).toBe(11);
  });

  test("reads the string literal under the cursor", () => {
    const line = `import Foo from "./Foo.vue"; import 'x'`;
    expect(stringLiteralAt(line, 20)).toBe("./Foo.vue");
    expect(stringLiteralAt(line, 3)).toBeNull();
    expect(stringLiteralAt(line, 38)).toBe("x");
  });

  test("resolves relative, alias and index imports", () => {
    const files = new Set(["src/components/Foo.vue", "src/utils/index.ts", "src/a.css"]);
    const exists = (path: string) => files.has(path);
    expect(resolveImportSpecifier("../components/Foo.vue", "src/views/Home.vue", exists)).toBe(
      "src/components/Foo.vue",
    );
    expect(resolveImportSpecifier("@/utils", "src/views/Home.vue", exists)).toBe("src/utils/index.ts");
    expect(resolveImportSpecifier("./a.css?inline", "src/main.ts", exists)).toBe("src/a.css");
    expect(resolveImportSpecifier("vue", "src/main.ts", exists)).toBeNull();
    expect(resolveImportSpecifier("../../../x", "src/main.ts", exists)).toBeNull();
  });

  test("maps component tags to file names but ignores native tags", () => {
    expect(componentFileNames("my-button")).toEqual(["MyButton.vue", "my-button.vue"]);
    expect(componentFileNames("MyButton")).toEqual(["MyButton.vue", "my-button.vue"]);
    expect(componentFileNames("button")).toEqual([]);
  });

  test("searches within the language family", () => {
    expect(searchIncludeFor("src/a.vue")).toContain("*.ts");
    expect(searchIncludeFor("src/lib.rs")).toBe("*.rs");
    expect(searchIncludeFor("Makefile")).toBe("");
  });

  test("normalizes Windows drive letters", () => {
    expect(normalizeDriveLetter("h:\\repo\\a.rs")).toBe("H:/repo/a.rs");
    expect(normalizeDriveLetter("/home/a.rs")).toBe("/home/a.rs");
  });
});
