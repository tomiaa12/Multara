// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  parseQuickOpenQuery,
  prepareQuickOpenFiles,
  rankPreparedQuickOpenFiles,
  rankQuickOpenFiles,
  scoreQuickOpenPath,
} from "./quick-open";

describe("parseQuickOpenQuery", () => {
  test("keeps a line suffix out of the file filter", () => {
    expect(parseQuickOpenQuery("views/index.vue:12")).toEqual({
      filter: "views/index.vue",
      startLine: 12,
      endLine: 12,
    });
    expect(parseQuickOpenQuery("index.vue:12-40")).toEqual({
      filter: "index.vue",
      startLine: 12,
      endLine: 40,
    });
  });

  test("treats a query without a line as the whole filter", () => {
    expect(parseQuickOpenQuery("views/index")).toEqual({ filter: "views/index" });
  });
});

describe("rankQuickOpenFiles", () => {
  const files = [
    { workspaceId: "app", path: "src/views/index.vue", label: "index.vue", detail: "src/views" },
    { workspaceId: "app", path: "src/pages/index.vue", label: "index.vue", detail: "src/pages" },
    { workspaceId: "app", path: "README.md", label: "README.md", detail: "" },
  ];

  test("matches a directory prefix that ends with a slash", () => {
    expect(rankQuickOpenFiles(files, "views/").map((file) => file.path)).toEqual([
      "src/views/index.vue",
    ]);
  });

  test("prefers a path that contains every typed piece", () => {
    expect(rankQuickOpenFiles(files, "views index").map((file) => file.path)).toEqual([
      "src/views/index.vue",
    ]);
  });

  test("ranks prepared candidates the same as raw files", () => {
    const candidates = prepareQuickOpenFiles(files);
    expect(rankPreparedQuickOpenFiles(candidates, "INDEX")).toEqual(rankQuickOpenFiles(files, "index"));
    expect(candidates[0]).toMatchObject({ hay: "src/views/index.vue", name: "index.vue" });
  });

  test("scores a file-name hit above a directory-only hit", () => {
    expect(scoreQuickOpenPath("src/views/index.vue", "index")).toBeGreaterThan(
      scoreQuickOpenPath("src/index/readme.md", "index") ?? 0,
    );
  });
});
