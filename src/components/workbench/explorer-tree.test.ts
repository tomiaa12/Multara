// @vitest-environment node
import { describe, expect, test } from "vitest";
import type { FileEntry } from "../../domain/types";
import { nestExplorerRows } from "./explorer-tree";

function entry(path: string, isDirectory: boolean, depth: number): FileEntry {
  const name = path.split("/").pop() ?? path;
  return { name, path, isDirectory, depth };
}

describe("nestExplorerRows", () => {
  test("nests each expanded folder as its own level and keeps sibling order", () => {
    const rows = [
      entry("src", true, 0),
      entry("src/components", true, 1),
      entry("src/components/chat", true, 2),
      entry("src/components/chat/composer-content.ts", false, 3),
      entry("src/components/ExploreGroup.vue", false, 2),
      entry("package.json", false, 0),
    ];
    const expanded = new Set(["src", "src/components", "src/components/chat"]);
    const tree = nestExplorerRows(rows, (item) => expanded.has(item.path));

    expect(tree.map((node) => node.entry.path)).toEqual(["src", "package.json"]);
    expect(tree[0]?.expanded).toBe(true);
    expect(tree[0]?.children.map((node) => node.entry.path)).toEqual(["src/components"]);
    expect(tree[0]?.children[0]?.children.map((node) => node.entry.path)).toEqual([
      "src/components/chat",
      "src/components/ExploreGroup.vue",
    ]);
    expect(tree[0]?.children[0]?.children[0]?.children.map((node) => node.entry.name)).toEqual([
      "composer-content.ts",
    ]);
    expect(tree[1]?.expanded).toBe(false);
  });

  test("leaves a row at the root when its parent folder is not in the list", () => {
    const rows = [entry("README.md", false, 0), entry("src/missing.ts", false, 1)];
    const tree = nestExplorerRows(rows, () => false);
    expect(tree.map((node) => node.entry.path)).toEqual(["README.md", "src/missing.ts"]);
  });
});
