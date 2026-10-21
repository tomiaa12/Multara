import { describe, expect, test } from "vitest";
import {
  decodeDroppedFileUrl,
  listOsDroppedEntries,
  preferDroppedFolders,
  resolvePathInWorkspaces,
} from "./workspace-file-drag";

describe("preferDroppedFolders", () => {
  test("keeps files when no folder was dropped", () => {
    expect(
      preferDroppedFolders([
        { path: "/repo/a.md", name: "a.md" },
        { path: "/repo/b.ts", name: "b.ts", isDirectory: false },
      ]),
    ).toEqual([
      { path: "/repo/a.md", name: "a.md" },
      { path: "/repo/b.ts", name: "b.ts", isDirectory: false },
    ]);
  });

  test("drops files that already sit inside a dropped folder", () => {
    expect(
      preferDroppedFolders([
        { path: "/repo/src", name: "src", isDirectory: true },
        { path: "/repo/src/app.ts", name: "app.ts", isDirectory: false },
        { path: "/repo/readme.md", name: "readme.md", isDirectory: false },
      ]),
    ).toEqual([
      { path: "/repo/src", name: "src", isDirectory: true },
      { path: "/repo/readme.md", name: "readme.md", isDirectory: false },
    ]);
  });

  test("keeps nameless-path files even when a folder is also dropped", () => {
    expect(
      preferDroppedFolders([
        { path: "", name: "docs", isDirectory: true },
        { path: "", name: "note.md", isDirectory: false },
      ]),
    ).toEqual([
      { path: "", name: "docs", isDirectory: true },
      { path: "", name: "note.md", isDirectory: false },
    ]);
  });
});

describe("decodeDroppedFileUrl", () => {
  test("reads mac file urls and private prefixes", () => {
    expect(decodeDroppedFileUrl("file:///Users/me/repo/a.md")).toBe("/Users/me/repo/a.md");
    expect(decodeDroppedFileUrl("file:///private/var/folders/xx/a.md")).toBe("/var/folders/xx/a.md");
  });
});

describe("resolvePathInWorkspaces", () => {
  const workspaces = [{ id: "ws", workPath: "/var/folders/xx/work", sourcePath: "/Users/me/repo" }];

  test("matches worktree and source roots", () => {
    expect(resolvePathInWorkspaces("/Users/me/repo/src/app.ts", workspaces)).toEqual({
      workspaceId: "ws",
      path: "src/app.ts",
    });
    expect(resolvePathInWorkspaces("/private/var/folders/xx/work/readme.md", workspaces)).toEqual({
      workspaceId: "ws",
      path: "readme.md",
    });
  });

  test("leaves outside-workspace paths unresolved", () => {
    expect(resolvePathInWorkspaces("/Users/me/Downloads/spec.md", workspaces)).toBeNull();
  });
});

function droppedData(files: File[], uriList = "") {
  const items = files.map((file) => ({
    getAsFile: () => file,
    webkitGetAsEntry: () => null,
  }));
  return {
    files,
    items,
    getData: (type: string) => (type === "text/uri-list" ? uriList : ""),
    types: ["Files"],
  } as unknown as DataTransfer;
}

describe("listOsDroppedEntries", () => {
  test("keeps files that have no native path so their content can still be read", () => {
    const file = new File(["# 大纲"], "spec.md", { type: "text/markdown" });
    const entries = listOsDroppedEntries(droppedData([file]));
    expect(entries).toHaveLength(1);
    expect(entries[0]?.path).toBe("");
    expect(entries[0]?.name).toBe("spec.md");
    expect(entries[0]?.file).toBe(file);
  });

  test("fills empty File.path from a file uri list", () => {
    const file = new File(["# 大纲"], "spec.md", { type: "text/markdown" });
    const entries = listOsDroppedEntries(
      droppedData([file], "file:///Users/me/Downloads/spec.md"),
    );
    expect(entries[0]?.path).toBe("/Users/me/Downloads/spec.md");
    expect(entries[0]?.file).toBe(file);
  });
});
