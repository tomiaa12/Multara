// @vitest-environment node
import { describe, expect, test } from "vitest";
import { gitBaselineText, gitChangeAtLine, gitGutterClass, gitGutterHover, gitGutterMarks, gitGutterModel, gitPeekBodyLines } from "./git-gutter-marks";

describe("gitGutterMarks", () => {
  test("marks added lines solid, edited lines modified, and a deletion between line numbers", () => {
    const baseline = ["a", "b", "c", "d"].join("\n");
    const current = ["a", "b2", "c", "added", "d"].join("\n");
    const marks = gitGutterMarks(baseline, current, 5);
    expect(marks).toEqual([
      { kind: "modified", line: 2, atEnd: false },
      { kind: "added", line: 4, atEnd: false },
    ]);
  });

  test("puts a deletion triangle on the following line", () => {
    const marks = gitGutterMarks("a\nb\nc\n", "a\nc\n", 2);
    expect(marks).toEqual([{ kind: "deleted", line: 2, atEnd: false }]);
  });

  test("pins a deletion after the last line to the end of the file", () => {
    const marks = gitGutterMarks("a\nb\n", "a\n", 1);
    expect(marks).toEqual([{ kind: "deleted", line: 1, atEnd: true }]);
  });

  test("treats a tracked empty file as all added", () => {
    const marks = gitGutterMarks("", "one\ntwo\n", 2);
    expect(marks.map((mark) => mark.kind)).toEqual(["added", "added"]);
  });

  test("skips untracked and binary files, matching a missing quick diff", () => {
    expect(gitBaselineText({ inRepo: true, tracked: false, binary: false, content: "" })).toBeNull();
    expect(gitBaselineText({ inRepo: false, tracked: false, binary: false, content: "" })).toBeNull();
    expect(gitBaselineText({ inRepo: true, tracked: true, binary: true, content: "" })).toBeNull();
    expect(gitBaselineText({ inRepo: true, tracked: true, binary: false, content: "a\n" })).toBe("a\n");
  });

  test("uses the vscode gutter classes and short tooltips", () => {
    expect(gitGutterClass({ kind: "added", line: 1, atEnd: false })).toBe(
      "dirty-diff-glyph dirty-diff-added primary",
    );
    expect(gitGutterClass({ kind: "modified", line: 1, atEnd: false })).toBe(
      "dirty-diff-glyph dirty-diff-modified primary pattern",
    );
    expect(gitGutterHover("added")).toBe("已添加的行");
    expect(gitGutterHover("modified")).toBe("已更改的行");
    expect(gitGutterHover("deleted")).toBe("已删除的行");
  });

  test("groups a hunk so a gutter click can open that change", () => {
    const { changes } = gitGutterModel(["a", "b", "c", "d"].join("\n"), ["a", "b2", "c", "added", "d"].join("\n"), 5);
    expect(changes.map((change) => change.kind)).toEqual(["modified", "added"]);
    expect(gitChangeAtLine(changes, 2)).toBe(0);
    expect(gitChangeAtLine(changes, 4)).toBe(1);
    expect(gitChangeAtLine(changes, 1)).toBe(-1);
  });

  test("sizes the peek to the hunk instead of a tall empty editor", () => {
    expect(gitPeekBodyLines({ oldLines: ["a"], newLines: ["b"] })).toBe(2);
    expect(gitPeekBodyLines({ oldLines: ["gone"], newLines: [] })).toBe(1);
    expect(gitPeekBodyLines({ oldLines: [], newLines: ["b"] })).toBe(1);
    expect(gitPeekBodyLines({ oldLines: Array.from({ length: 20 }, () => "x"), newLines: [] })).toBe(10);
  });
});
