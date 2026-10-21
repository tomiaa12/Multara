import { beforeEach, describe, expect, test } from "vitest";
import type { GitRepositoryStatus } from "../../domain/types";
import { applyLanguageSetting } from "../../i18n";
import { branchAction, branchNameError, checkoutRows, renameBranchNotice, syncAction } from "./scm-branch";

beforeEach(() => {
  applyLanguageSetting("zh-CN");
});

function repo(partial: Partial<GitRepositoryStatus>): GitRepositoryStatus {
  return {
    workspaceId: "w",
    name: "app",
    path: "/app",
    branch: "main",
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    hasRemote: true,
    merging: false,
    changes: [],
    branches: ["main", "dev"],
    remoteBranches: ["origin/main"],
    ...partial,
  };
}

describe("scm branch bar", () => {
  test("marks a dirty and staged branch the way VS Code does", () => {
    const action = branchAction(repo({
      changes: [
        { path: "a.ts", indexStatus: " ", worktreeStatus: "M", staged: false },
        { path: "b.ts", indexStatus: "M", worktreeStatus: " ", staged: true },
      ],
    }));
    expect(action.label).toBe("main*+");
    expect(action.icon).toBe("git-branch-staged-changes");
    expect(action.tooltip).toBe("main*+，检出分支或标记…");
  });

  test("sync text and publish icon follow the upstream", () => {
    expect(syncAction(repo({ ahead: 2, behind: 1 }))).toMatchObject({
      icon: "sync",
      text: "1↓ 2↑",
      kind: "sync",
    });
    expect(syncAction(repo({ upstream: null }))).toMatchObject({ icon: "cloud-upload", kind: "publish" });
    expect(syncAction(repo({ hasRemote: false, upstream: null }))).toBeNull();
  });

  test("checkout quick pick puts commands first until the query filters refs", () => {
    const idle = checkoutRows(repo({}), "");
    expect(idle[0]).toMatchObject({ id: "create" });
    expect(idle.find((row) => row.type === "sep" && row.label === "远程分支")).toBeTruthy();
    const filtered = checkoutRows(repo({}), "origin");
    expect(filtered.find((row) => row.type === "branch")).toMatchObject({ name: "origin/main", remote: true });
    expect(filtered.at(-1)).toMatchObject({ id: "detach" });
    expect(branchNameError("feature/test")).toBe("");
    expect(branchNameError("bad name")).toBe("分支名称无效");
  });

  test("rename notice states local, remote, and the new upstream", () => {
    expect(renameBranchNotice({
      localRenamed: true,
      localBranch: "feature",
      remoteAttempted: true,
      remoteOk: true,
      remoteError: "",
      hadUpstream: true,
      upstream: "origin/feature",
    })).toBe("本地分支已改为 feature。远程分支已改名并完成推送。上游已设为 origin/feature。");
    expect(renameBranchNotice({
      localRenamed: true,
      localBranch: "feature",
      remoteAttempted: false,
      remoteOk: false,
      remoteError: "",
      hadUpstream: true,
      upstream: "",
    })).toContain("上游已取消");
  });
});