// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  editorConfigSearchPaths,
  editorOptionsFromConfig,
  matchEditorConfig,
  parseEditorConfig,
} from "./editorconfig";

describe("editorconfig", () => {
  test("walks from workspace root to the file directory", () => {
    expect(editorConfigSearchPaths("src/app/main.ts")).toEqual([
      ".editorconfig",
      "src/.editorconfig",
      "src/app/.editorconfig",
    ]);
  });

  test("applies the last matching section", () => {
    const sections = parseEditorConfig(`
[*]
indent_style = space
indent_size = 2
end_of_line = lf

[*.md]
indent_size = 4
`);
    expect(editorOptionsFromConfig(matchEditorConfig("README.md", sections))).toEqual({
      insertSpaces: true,
      tabSize: 4,
      eol: "LF",
    });
    expect(editorOptionsFromConfig(matchEditorConfig("src/a.ts", sections)).tabSize).toBe(2);
  });
});
