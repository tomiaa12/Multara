// @vitest-environment node
import { describe, expect, test } from "vitest";
import type { Workspace } from "../../domain/types";
import {
  branchNameFromRequirement,
  defaultNewBranchName,
  detachWorkspacePrompt,
  requirementBranchNames,
  workspaceBranchName,
} from "./workspace-mode";

function workspace(patch: Partial<Workspace> & Pick<Workspace, "id" | "projectId" | "kind">): Workspace {
  return {
    sessionId: null,
    sourceWorkspaceId: null,
    name: patch.id,
    sourcePath: "/src",
    workPath: "/work",
    branch: null,
    isolationMode: "worktree",
    createdAt: "2026-09-23T00:00:00Z",
    ...patch,
  };
}

describe("workspaceBranchName", () => {
  test("prefers the live branch", () => {
    const item = workspace({ id: "a", projectId: "p", kind: "git", branch: "stored" });
    expect(workspaceBranchName(item, "live")).toBe("live");
  });

  test("falls back to the stored branch", () => {
    const item = workspace({ id: "a", projectId: "p", kind: "git", branch: "stored" });
    expect(workspaceBranchName(item, "  ")).toBe("stored");
  });
});

describe("requirementBranchNames", () => {
  test("keeps this requirement's git branches in order and drops duplicates", () => {
    const names = requirementBranchNames(
      [
        workspace({ id: "a", projectId: "p", kind: "git", branch: "feat/one" }),
        workspace({ id: "b", projectId: "p", kind: "folder", branch: null }),
        workspace({ id: "c", projectId: "other", kind: "git", branch: "feat/other" }),
        workspace({ id: "d", projectId: "p", kind: "git", branch: "feat/one" }),
        workspace({ id: "e", projectId: "p", kind: "git", branch: "feat/two" }),
      ],
      "p",
    );
    expect(names).toEqual(["feat/one", "feat/two"]);
  });
});

describe("branchNameFromRequirement", () => {
  test("prefixes a slugified requirement name with feat/", () => {
    expect(branchNameFromRequirement("Login Page")).toBe("feat/login-page");
  });

  test("keeps chinese characters and strips invalid git tokens", () => {
    expect(branchNameFromRequirement("  登录页改版？ ")).toBe("feat/登录页改版");
  });

  test("falls back when the name is empty after sanitizing", () => {
    expect(branchNameFromRequirement("???")).toBe("feat/feature");
  });

  test("uses the configured prefix", () => {
    expect(branchNameFromRequirement("登录页改版", "fix")).toBe("fix/登录页改版");
    expect(branchNameFromRequirement("登录页改版", "Hot Fix!")).toBe("hotfix/登录页改版");
  });
});

describe("defaultNewBranchName", () => {
  test("selects the first known branch", () => {
    expect(defaultNewBranchName("登录页改版", ["feat/one", "feat/two"])).toBe("feat/one");
  });

  test("uses feat/requirement when this requirement has no git branch yet", () => {
    expect(defaultNewBranchName("登录页改版", [])).toBe("feat/登录页改版");
  });
});

describe("detachWorkspacePrompt", () => {
  const base = {
    name: "Multara",
    branch: "main",
    unsavedCount: 0,
    sharedCheckout: false,
  };

  test("direct git with uncommitted changes only unlinks the real folder", () => {
    const text = detachWorkspacePrompt({
      ...base,
      mode: "direct",
      isolationMode: "direct",
      dirtyCount: 3,
    });
    expect(text).toContain("3 个未提交更改");
    expect(text).toContain("都会保留");
    expect(text).toContain("确定移除？");
  });

  test("a clean worktree says the checkout will be deleted and the branch stays", () => {
    const text = detachWorkspacePrompt({
      ...base,
      mode: "worktree",
      isolationMode: "worktree",
      dirtyCount: 0,
    });
    expect(text).toContain("独立工作树目录会被删除");
    expect(text).toContain("分支会留在原仓库里");
  });

  test("a dirty worktree warns that uncommitted work is lost", () => {
    const text = detachWorkspacePrompt({
      ...base,
      mode: "worktree",
      isolationMode: "worktree",
      dirtyCount: 2,
      unsavedCount: 1,
    });
    expect(text).toContain("会丢失");
    expect(text).toContain("1 个未保存文件");
  });

  test("a plain folder never claims the real directory will be deleted", () => {
    const text = detachWorkspacePrompt({
      ...base,
      mode: "folder",
      isolationMode: "direct",
      dirtyCount: null,
      branch: "",
    });
    expect(text).toContain("磁盘上的文件会保留");
  });
});
