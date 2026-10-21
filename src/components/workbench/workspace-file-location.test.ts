import { describe, expect, test } from "vitest";
import {
  filePathMatchesReference,
  isBareFileName,
  matchLongestWorkspaceRoot,
  pathWithoutWorktreeDir,
  workspaceFileLabel,
} from "./workspace-file-location";

const workspaces = [
  { id: "client", name: "client", roots: ["/repo/client", "/src/client"] },
  { id: "admin", name: "admin", roots: ["/repo/admin"] },
  { id: "nested", name: "nested", roots: ["/repo/client/packages/app"] },
];

describe("matchLongestWorkspaceRoot", () => {
  test("uses the longest root prefix", () => {
    expect(
      matchLongestWorkspaceRoot("/repo/client/packages/app/src/main.ts", workspaces),
    ).toEqual({ workspaceId: "nested", path: "src/main.ts" });
  });

  test("keeps a file under the shorter root when it is outside the nested one", () => {
    expect(
      matchLongestWorkspaceRoot("/repo/client/docs/CLIENT_DETAIL_IMPLEMENTATION.md", workspaces),
    ).toEqual({ workspaceId: "client", path: "docs/CLIENT_DETAIL_IMPLEMENTATION.md" });
  });

  test("ignores an absolute path outside every root", () => {
    expect(matchLongestWorkspaceRoot("/other/file.md", workspaces)).toBeNull();
  });
});

describe("workspace file labels", () => {
  test("prefixes the project name when several roots are open", () => {
    expect(workspaceFileLabel("client", "docs/guide.md", true)).toBe("client • docs/guide.md");
  });

  test("shows only the relative path for a single root", () => {
    expect(workspaceFileLabel("client", "docs/guide.md", false)).toBe("docs/guide.md");
  });

  test("treats a slash-free string as a file name", () => {
    expect(isBareFileName("CLIENT_DETAIL_IMPLEMENTATION.md")).toBe(true);
    expect(isBareFileName("docs/CLIENT_DETAIL_IMPLEMENTATION.md")).toBe(false);
  });
});

describe("filePathMatchesReference", () => {
  test("matches a shorter path that is a suffix of the workspace file", () => {
    expect(filePathMatchesReference("src/views/index.vue", "views/index.vue")).toBe(true);
    expect(filePathMatchesReference("src/views/index.vue", "./views/index.vue")).toBe(true);
  });

  test("does not match a different directory with the same file name", () => {
    expect(filePathMatchesReference("src/pages/index.vue", "views/index.vue")).toBe(false);
  });
});

describe("pathWithoutWorktreeDir", () => {
  test("drops a leading worktree directory", () => {
    expect(pathWithoutWorktreeDir(
      "a958e6d9-c8c4-4e46-b5bb-3c540e4dc1d2/internal/uc/service/user/user_info.go",
    )).toBe("internal/uc/service/user/user_info.go");
  });
});
