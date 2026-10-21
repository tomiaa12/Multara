// @vitest-environment node
import { describe, expect, test } from "vitest";
import type { GitFileStatus } from "../../domain/types";
import { scmFileTree, scmGroups, scmResourceLetter } from "./scm-groups";

function file(path: string, indexStatus: string, worktreeStatus: string): GitFileStatus {
  return {
    path,
    indexStatus,
    worktreeStatus,
    staged: indexStatus !== " " && indexStatus !== "?",
  };
}

describe("scmGroups", () => {
  test("puts a staged-and-modified file in both staged and changes", () => {
    const groups = scmGroups([file("src/app.ts", "M", "M")]);
    expect(groups.staged.map((item) => item.path)).toEqual(["src/app.ts"]);
    expect(groups.changes.map((item) => item.path)).toEqual(["src/app.ts"]);
    expect(groups.merge).toHaveLength(0);
    expect(groups.untracked).toHaveLength(0);
  });

  test("keeps conflicts out of the other groups", () => {
    const groups = scmGroups([
      file("src/app.ts", "U", "U"),
      file("src/both.ts", "A", "A"),
      file("src/new.ts", "A", " "),
      file("src/edit.ts", " ", "M"),
      file("notes.md", "?", "?"),
    ]);
    expect(groups.merge.map((item) => item.path)).toEqual(["src/app.ts", "src/both.ts"]);
    expect(groups.staged.map((item) => item.path)).toEqual(["src/new.ts"]);
    expect(groups.changes.map((item) => item.path)).toEqual(["src/edit.ts"]);
    expect(groups.untracked.map((item) => item.path)).toEqual(["notes.md"]);
  });

  test("letters follow the group side", () => {
    const both = file("src/app.ts", "A", "M");
    expect(scmResourceLetter(both, "staged")).toBe("A");
    expect(scmResourceLetter(both, "changes")).toBe("M");
    expect(scmResourceLetter(file("src/app.ts", "U", "U"), "merge")).toBe("C");
  });
});

describe("scmFileTree", () => {
  test("nests files under directories and counts them", () => {
    const tree = scmFileTree([
      file("src/a.ts", " ", "M"),
      file("src/nested/b.ts", " ", "M"),
      file("README.md", " ", "M"),
    ]);
    expect(tree.map((item) => item.kind === "dir" ? item.name : item.path)).toEqual(["src", "README.md"]);
    const src = tree[0];
    expect(src.kind).toBe("dir");
    if (src.kind === "dir") {
      expect(src.count).toBe(2);
      expect(src.children.map((item) => (item.kind === "dir" ? item.name : item.path))).toEqual([
        "nested",
        "src/a.ts",
      ]);
    }
  });
});
