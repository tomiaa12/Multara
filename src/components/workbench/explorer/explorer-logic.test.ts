// @vitest-environment node
import { describe, expect, test } from "vitest";
import { compareExplorerEntries, sortExplorerEntries } from "./explorer-compare";
import { canDropInExplorer, dropFolderFor, isCopyDrag } from "./explorer-dnd";
import { globMatch, isExcluded } from "./explorer-glob";
import { findAvailableName, incrementFileName, renameSelection, validateFileName } from "./explorer-names";
import { ExplorerFileNestingTrie, nestDirectoryFiles } from "./explorer-nesting";
import { applyFind, buildExplorerRows, flattenRows, fuzzyMatch, type BuildRowsInput } from "./explorer-rows";
import {
  distinctParents,
  moveFocus,
  pruneSelection,
  selectOnly,
  selectRange,
  toggleSelection,
  typeNavigate,
} from "./explorer-selection";
import { explorerHoverTitle, formatByteSize, highlightName } from "./explorer-format";
import { DEFAULT_EXPLORER_SETTINGS, applySeparator, filesExcludeFromText, filesExcludeToText, parseExplorerSettings, serializeExplorerSettings } from "./explorer-settings";
import { ExplorerUndoStack } from "./explorer-undo";

describe("incrementFileName (VS Code fileActions.test.ts)", () => {
  test.each([
    ["test.js", false, "test copy.js"],
    ["test copy.js", false, "test copy 2.js"],
    ["test copy 005.js", false, "test copy 6.js"],
    ["copy.js", false, "copy copy.js"],
    ["copy 2.js", false, "copy 2 copy.js"],
    ["test", false, "test copy"],
    ["test.", false, "test copy."],
    [".test", false, ".test copy"],
    ["..test", false, ". copy.test"],
    ["test copy 5", false, "test copy 6"],
    ["test", true, "test copy"],
    ["test.js", true, "test.js copy"],
    ["test copy 0", true, "test copy"],
    ["test copy 1", true, "test copy 2"],
  ])("simple: %s (folder=%s) -> %s", (name, isFolder, expected) => {
    expect(incrementFileName(name, isFolder, "simple")).toBe(expected);
  });

  test.each([
    ["test.js", false, "test.1.js"],
    ["test", true, "test.1"],
    ["test.1.js", false, "test.2.js"],
    ["test.009.js", false, "test.010.js"],
    ["test-099.js", false, "test-100.js"],
    ["test_1.js", false, "test_2.js"],
    ["test.001", true, "test.002"],
    ["1.test.js", false, "2.test.js"],
    ["1.js", false, "2.js"],
    ["123", false, "124"],
    ["file", false, "file1"],
    ["1.folder", true, "2.folder"],
  ])("smart: %s (folder=%s) -> %s", (name, isFolder, expected) => {
    expect(incrementFileName(name, isFolder, "smart")).toBe(expected);
  });

  test("findAvailableName keeps incrementing until free", () => {
    const taken = new Set(["a.ts", "a copy.ts", "a copy 2.ts"]);
    expect(findAvailableName("a.ts", false, "simple", (name) => taken.has(name))).toBe("a copy 3.ts");
    expect(findAvailableName("a.ts", false, "disabled", (name) => taken.has(name))).toBe("a.ts");
  });
});

describe("validateFileName", () => {
  const exists = (names: string[]) => (name: string) => names.includes(name);
  test("rejects empty, leading slash, duplicates and invalid characters", () => {
    expect(validateFileName("   ", { siblingExists: exists([]), isWindows: true })?.severity).toBe("error");
    expect(validateFileName("/a", { siblingExists: exists([]), isWindows: true })?.message).toContain("斜杠");
    expect(validateFileName("a.ts", { siblingExists: exists(["a.ts"]), isWindows: true })?.message).toContain("已存在");
    expect(validateFileName("a.ts", { currentName: "a.ts", siblingExists: exists(["a.ts"]), isWindows: true })).toBeNull();
    expect(validateFileName("a?.ts", { siblingExists: exists([]), isWindows: true })?.severity).toBe("error");
    expect(validateFileName("a?.ts", { siblingExists: exists([]), isWindows: false })).toBeNull();
    expect(validateFileName("con.txt", { siblingExists: exists([]), isWindows: true })?.severity).toBe("error");
  });

  test("allows nested paths and warns about surrounding whitespace on POSIX", () => {
    expect(validateFileName("src/new/a.ts", { siblingExists: exists(["src"]), isWindows: true })).toBeNull();
    expect(validateFileName(" a.ts", { siblingExists: exists([]), isWindows: false })?.severity).toBe("warning");
  });

  test("rename selects the stem only for files", () => {
    expect(renameSelection("index.test.ts", false)).toEqual([0, 10]);
    expect(renameSelection(".gitignore", false)).toEqual([0, 10]);
    expect(renameSelection("src.v2", true)).toEqual([0, 6]);
  });
});

describe("comparers (VS Code FileSorter)", () => {
  const options = { sortOrder: "default", lexicographic: "default", reverse: false } as const;
  const names = (list: Array<{ name: string }>) => list.map((item) => item.name);

  test("default puts folders first and sorts numerically", () => {
    const sorted = sortExplorerEntries(
      [
        { name: "file10.ts", isDirectory: false },
        { name: "file2.ts", isDirectory: false },
        { name: "src", isDirectory: true },
        { name: "B.ts", isDirectory: false },
        { name: "a.ts", isDirectory: false },
      ],
      options,
    );
    expect(names(sorted)).toEqual(["src", "a.ts", "B.ts", "file2.ts", "file10.ts"]);
  });

  test("filesFirst, mixed, type, modified and reverse", () => {
    const entries = [
      { name: "z", isDirectory: true, mtime: 1 },
      { name: "b.md", isDirectory: false, mtime: 3 },
      { name: "a.ts", isDirectory: false, mtime: 2 },
      { name: "c.md", isDirectory: false, mtime: 5 },
    ];
    expect(names(sortExplorerEntries(entries, { ...options, sortOrder: "filesFirst" }))).toEqual(["a.ts", "b.md", "c.md", "z"]);
    expect(names(sortExplorerEntries(entries, { ...options, sortOrder: "mixed" }))).toEqual(["a.ts", "b.md", "c.md", "z"]);
    expect(names(sortExplorerEntries(entries, { ...options, sortOrder: "type" }))).toEqual(["z", "b.md", "c.md", "a.ts"]);
    expect(names(sortExplorerEntries(entries, { ...options, sortOrder: "modified" }))).toEqual(["z", "c.md", "b.md", "a.ts"]);
    expect(names(sortExplorerEntries(entries, { ...options, reverse: true }))).toEqual(["c.md", "b.md", "a.ts", "z"]);
  });

  test("upper and lower lexicographic options group by case", () => {
    const entries = [
      { name: "b", isDirectory: false },
      { name: "A", isDirectory: false },
      { name: "a", isDirectory: false },
      { name: "B", isDirectory: false },
    ];
    expect(names(sortExplorerEntries(entries, { ...options, lexicographic: "upper" }))).toEqual(["A", "B", "a", "b"]);
    expect(names(sortExplorerEntries(entries, { ...options, lexicographic: "lower" }))).toEqual(["a", "b", "A", "B"]);
    expect(compareExplorerEntries({ name: "a", isDirectory: false }, { name: "B", isDirectory: false }, { ...options, lexicographic: "unicode" })).toBe(1);
  });
});

describe("file nesting trie (VS Code explorerFileNestingTrie.test.ts)", () => {
  test("nests captures and simple names", () => {
    const trie = new ExplorerFileNestingTrie([
      ["*.ts", ["${capture}.js"]],
      ["*.js", ["${capture}.map"]],
    ]);
    const nesting = trie.nest(["a.ts", "a.js", "a.map", "b.js", "b.map"], "");
    expect([...nesting.keys()].sort()).toEqual(["a.ts", "b.js"]);
    expect([...nesting.get("a.ts")!].sort()).toEqual(["a.js", "a.map"]);
  });

  test("supports basename, extname and dirname", () => {
    const trie = new ExplorerFileNestingTrie([
      ["*", ["${basename}.*.${extname}"]],
      ["index.ts", ["${dirname}.ts"]],
    ]);
    const nesting = trie.nest(["a.ts", "a.b.ts", "index.ts", "src.ts"], "src");
    expect([...nesting.get("a.ts")!]).toEqual(["a.b.ts"]);
    expect([...nesting.get("index.ts")!]).toEqual(["src.ts"]);
  });

  test("default patterns nest lock files under package.json", () => {
    const nests = nestDirectoryFiles(
      ["package.json", "pnpm-lock.yaml", "tsconfig.json", "tsconfig.node.json", "main.ts"],
      "",
      DEFAULT_EXPLORER_SETTINGS.fileNestingPatterns,
    );
    expect(nests.get("package.json")).toEqual(["pnpm-lock.yaml"]);
    expect(nests.get("tsconfig.json")).toEqual(["tsconfig.node.json"]);
    expect(nests.has("main.ts")).toBe(false);
  });
});

describe("glob exclude", () => {
  test("matches VS Code style patterns", () => {
    expect(globMatch("**/.git", ".git")).toBe(true);
    expect(globMatch("**/.git", "a/b/.git")).toBe(true);
    expect(globMatch("**/.git", "a/.gitignore")).toBe(false);
    expect(globMatch("*.log", "a.log")).toBe(true);
    expect(globMatch("*.log", "src/a.log")).toBe(false);
    expect(globMatch("**/*.{js,map}", "out/x.map")).toBe(true);
    expect(globMatch("dist/**", "dist/a/b.js")).toBe(true);
    expect(globMatch("file[0-9].txt", "file3.txt")).toBe(true);
    expect(globMatch("file[!0-9].txt", "file3.txt")).toBe(false);
  });

  test("when clause checks siblings", () => {
    const expression = { "**/*.js": { when: "$(basename).ts" } };
    expect(isExcluded("src/a.js", "a.js", expression, (name) => name === "a.ts")).toBe(true);
    expect(isExcluded("src/b.js", "b.js", expression, (name) => name === "a.ts")).toBe(false);
    expect(isExcluded("x", "x", { "**/x": false }, () => false)).toBe(false);
  });
});

describe("selection model", () => {
  const order = ["w:", "w:a", "w:b", "w:c", "w:d"];
  test("single, toggle, range and additive range", () => {
    let state = selectOnly("w:a");
    state = toggleSelection(state, "w:c");
    expect(state.selected).toEqual(["w:a", "w:c"]);
    state = selectRange(state, order, "w:d");
    expect(state.selected).toEqual(["w:c", "w:d"]);
    state = selectRange(selectOnly("w:a"), order, "w:b", true);
    expect(state.selected).toEqual(["w:a", "w:b"]);
  });

  test("focus moves clamp to the ends", () => {
    expect(moveFocus(order, "w:d", "next")).toBe("w:d");
    expect(moveFocus(order, "w:a", "previous")).toBe("w:");
    expect(moveFocus(order, null, "next")).toBe("w:");
    expect(moveFocus(order, "w:a", "pageDown", 2)).toBe("w:c");
    expect(moveFocus(order, "w:a", "last")).toBe("w:d");
  });

  test("prune keeps focus near the removed item", () => {
    const pruned = pruneSelection({ selected: ["w:b"], focus: "w:b", anchor: "w:b" }, ["w:", "w:a", "w:c"], order);
    expect(pruned).toEqual({ selected: [], focus: "w:a", anchor: "w:a" });
  });

  test("distinctParents drops children of selected folders", () => {
    const items = [
      { workspaceId: "w", path: "src" },
      { workspaceId: "w", path: "src/a.ts" },
      { workspaceId: "x", path: "src/a.ts" },
    ];
    expect(distinctParents(items)).toEqual([items[0], items[2]]);
  });

  test("type navigation cycles through matching names", () => {
    const names = new Map([
      ["w:a", "alpha"],
      ["w:b", "beta"],
      ["w:c", "apple"],
    ]);
    const keys = ["w:a", "w:b", "w:c"];
    expect(typeNavigate(keys, names, "w:a", "be")).toBe("w:b");
    expect(typeNavigate(keys, names, "w:a", "aa")).toBe("w:c");
    expect(typeNavigate(keys, names, null, "z")).toBeNull();
  });
});

describe("drag and drop rules (VS Code FileDragAndDrop)", () => {
  const src = (path: string, isDirectory = false) => ({ workspaceId: "w", path, isDirectory });
  test("rejects self, same parent move and descendants", () => {
    expect(canDropInExplorer([src("a", true)], { workspaceId: "w", path: "a" }, false)).toBe(false);
    expect(canDropInExplorer([src("a/b.ts")], { workspaceId: "w", path: "a" }, false)).toBe(false);
    expect(canDropInExplorer([src("a/b.ts")], { workspaceId: "w", path: "a" }, true)).toBe(true);
    expect(canDropInExplorer([src("a", true)], { workspaceId: "w", path: "a/b" }, true)).toBe(false);
    expect(canDropInExplorer([src("a/b.ts")], { workspaceId: "w", path: "" }, false)).toBe(true);
    expect(canDropInExplorer([src("a/b.ts")], { workspaceId: "x", path: "a" }, false)).toBe(true);
    expect(canDropInExplorer([{ ...src("a.ts"), readonly: true }], { workspaceId: "w", path: "b" }, false)).toBe(false);
  });

  test("drops on files bubble to the parent and copy uses the platform modifier", () => {
    expect(dropFolderFor({ path: "a/b.ts", isDirectory: false })).toBe("a");
    expect(dropFolderFor({ path: "a", isDirectory: true })).toBe("a");
    expect(isCopyDrag({ ctrlKey: true, altKey: false }, false)).toBe(true);
    expect(isCopyDrag({ ctrlKey: true, altKey: false }, true)).toBe(false);
    expect(isCopyDrag({ ctrlKey: false, altKey: true }, true)).toBe(true);
  });
});

describe("row building", () => {
  const base = (overrides: Partial<BuildRowsInput> = {}): BuildRowsInput => {
    const tree: Record<string, Array<{ name: string; path: string; isDirectory: boolean; depth: number }>> = {
      "": [
        { name: "src", path: "src", isDirectory: true, depth: 0 },
        { name: ".git", path: ".git", isDirectory: true, depth: 0 },
        { name: "package.json", path: "package.json", isDirectory: false, depth: 0 },
        { name: "pnpm-lock.yaml", path: "pnpm-lock.yaml", isDirectory: false, depth: 0 },
      ],
      src: [{ name: "main", path: "src/main", isDirectory: true, depth: 1 }],
      "src/main": [{ name: "app.ts", path: "src/main/app.ts", isDirectory: false, depth: 2 }],
    };
    return {
      workspaceId: "w",
      workspaceName: "w",
      roots: tree[""],
      childrenOf: (path) => tree[path],
      isExpanded: () => true,
      isNestExpanded: () => false,
      sort: { sortOrder: "default", lexicographic: "default", reverse: false },
      compactFolders: true,
      nestingPatterns: DEFAULT_EXPLORER_SETTINGS.fileNestingPatterns,
      isHidden: (entry, siblings) => isExcluded(entry.path, entry.name, DEFAULT_EXPLORER_SETTINGS.filesExclude, (name) => siblings.has(name)),
      forceVisible: () => false,
      breakCompactAt: () => false,
      missingFiles: [],
      ...overrides,
    };
  };

  test("compacts single-child folders, hides excluded and nests files", () => {
    const rows = flattenRows(buildExplorerRows(base()));
    expect(rows.map((row) => row.name)).toEqual(["src/main", "app.ts", "package.json"]);
    expect(rows[0].compact?.paths).toEqual(["src", "src/main"]);
    expect(rows[1].depth).toBe(1);
    expect(rows[2].nestParent).toBe(true);
  });

  test("expanded nests show children; compact chain breaks where editing", () => {
    const rows = flattenRows(buildExplorerRows(base({ isNestExpanded: () => true, breakCompactAt: (path) => path === "src" })));
    expect(rows.map((row) => row.name)).toEqual(["src", "main", "app.ts", "package.json", "pnpm-lock.yaml"]);
    expect(rows[4].nestedChild).toBe(true);
  });

  test("excluded entries open in an editor stay visible", () => {
    const rows = flattenRows(buildExplorerRows(base({ forceVisible: (path) => path === ".git", isExpanded: () => false })));
    expect(rows.find((row) => row.path === ".git")?.excluded).toBe(true);
  });

  test("missing files are inserted under expanded folders", () => {
    const rows = flattenRows(buildExplorerRows(base({ compactFolders: false, missingFiles: ["src/main/gone.ts", "x/y.ts"] })));
    expect(rows.some((row) => row.path === "src/main/gone.ts" && row.missing)).toBe(true);
    expect(rows.some((row) => row.path === "x/y.ts")).toBe(false);
  });

  test("find filters to matches and their ancestors", () => {
    const nodes = buildExplorerRows(base({ compactFolders: false }));
    const filtered = flattenRows(applyFind(nodes, "app", "filter"));
    expect(filtered.map((row) => row.name)).toEqual(["src", "main", "app.ts"]);
    expect(filtered[2].matches).toEqual([[0, 3]]);
    expect(fuzzyMatch("pjs", "package.json")).toEqual([[0, 1], [8, 10]]);
  });
});

describe("settings", () => {
  test("parse falls back to defaults and serialize stores only the diff", () => {
    const parsed = parseExplorerSettings('{"sortOrder":"type","compactFolders":"no","incrementalNaming":"bogus"}');
    expect(parsed.sortOrder).toBe("type");
    expect(parsed.compactFolders).toBe(true);
    expect(parsed.incrementalNaming).toBe("simple");
    expect(serializeExplorerSettings(parsed)).toBe('{"sortOrder":"type"}');
    expect(parseExplorerSettings("not json").confirmDelete).toBe(true);
  });

  test("path separators", () => {
    expect(applySeparator("a/b", "auto", true)).toBe("a\\b");
    expect(applySeparator("a\\b", "/", true)).toBe("a/b");
    expect(applySeparator("a/b", "auto", false)).toBe("a/b");
  });

  test("files.exclude text round-trips enabled globs", () => {
    const text = filesExcludeToText({ "**/.git": true, "tmp": { when: "$(basename).ts" }, "skip": false });
    expect(text.split("\n")).toEqual(["**/.git"]);
    expect(filesExcludeFromText("**/.git\n# comment\n\n**/dist")).toEqual({
      "**/.git": true,
      "**/dist": true,
    });
  });
});

describe("undo stack", () => {
  test("undo/redo order and clearing redo on push", async () => {
    const log: string[] = [];
    const op = (name: string) => ({
      label: name,
      destructiveUndo: false,
      undo: async () => void log.push(`undo ${name}`),
      redo: async () => void log.push(`redo ${name}`),
    });
    const stack = new ExplorerUndoStack();
    stack.push(op("a"));
    stack.push(op("b"));
    await stack.undo();
    await stack.redo();
    await stack.undo();
    stack.push(op("c"));
    expect(stack.canRedo).toBe(false);
    await stack.undo();
    expect(log).toEqual(["undo b", "redo b", "undo b", "undo c"]);
  });

  test("a failing undo clears history", async () => {
    const stack = new ExplorerUndoStack();
    stack.push({ label: "x", destructiveUndo: false, undo: async () => Promise.reject(new Error("boom")), redo: async () => undefined });
    await expect(stack.undo()).rejects.toThrow("boom");
    expect(stack.canUndo).toBe(false);
  });
});

describe("explorer hover and find highlight", () => {
  test("formats sizes and hover titles", () => {
    expect(formatByteSize(800)).toBe("800 B");
    expect(formatByteSize(2048)).toBe("2.0 KB");
    expect(explorerHoverTitle({ path: "src/a.ts", isDirectory: false, size: 2048 })).toContain("src/a.ts");
    expect(explorerHoverTitle({ path: "src/a.ts", isDirectory: false, size: 2048 })).toContain("2.0 KB");
  });

  test("highlights contiguous and fuzzy ranges", () => {
    expect(highlightName("index.ts", [[0, 5]])).toEqual([
      { text: "index", hit: true },
      { text: ".ts", hit: false },
    ]);
    expect(highlightName("App.vue")).toEqual([{ text: "App.vue", hit: false }]);
  });
});
