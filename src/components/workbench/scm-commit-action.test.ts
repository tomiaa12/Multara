// @vitest-environment node
import { beforeEach, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import {
  canPublishBranch,
  canSyncChanges,
  discardConfirmMessage,
  publishSkipsStagedChanges,
  publishWithoutCommitConfirm,
  scmPrimaryAction,
  stepCommitMessage,
  rememberCommitMessage,
  resetCommitMessageCursor,
} from "./scm-commit-action";

beforeEach(() => {
  applyLanguageSetting("zh-CN");
});

const published = { hasRemote: true, upstream: "origin/main", merging: false };
const unpublished = { hasRemote: true, upstream: null, merging: false };

describe("scmPrimaryAction", () => {
  test("unpublished branch with a remote uses 发布分支", () => {
    expect(canPublishBranch(unpublished)).toBe(true);
    expect(scmPrimaryAction(unpublished)).toBe("publish");
  });

  test("staged changes on an unpublished branch commit before publish", () => {
    expect(scmPrimaryAction(unpublished, true)).toBe("commit");
  });

  test("published branch keeps 提交", () => {
    expect(canPublishBranch(published)).toBe(false);
    expect(scmPrimaryAction(published)).toBe("commit");
  });

  test("no remote cannot publish", () => {
    expect(canPublishBranch({ hasRemote: false, upstream: null, merging: false })).toBe(false);
    expect(scmPrimaryAction({ hasRemote: false, upstream: null, merging: false })).toBe("commit");
  });

  test("merging cannot publish", () => {
    expect(canPublishBranch({ ...unpublished, merging: true })).toBe(false);
    expect(scmPrimaryAction({ ...unpublished, merging: true })).toBe("commit");
  });
});

describe("canSyncChanges", () => {
  const base = { hasRemote: true, upstream: "origin/main", merging: false, ahead: 0, behind: 0 };

  test("ahead only can sync", () => {
    expect(canSyncChanges({ ...base, ahead: 1 })).toBe(true);
  });

  test("behind only can sync", () => {
    expect(canSyncChanges({ ...base, behind: 2 })).toBe(true);
  });

  test("clean published branch does not sync", () => {
    expect(canSyncChanges(base)).toBe(false);
  });

  test("unpublished branch uses publish instead of sync", () => {
    expect(canSyncChanges({ ...base, upstream: null, ahead: 3 })).toBe(false);
  });

  test("after a commit the main button becomes 同步更改", () => {
    expect(scmPrimaryAction({ ...base, ahead: 1 }, false)).toBe("sync");
  });

  test("staged changes keep 提交 even when the branch is ahead", () => {
    expect(scmPrimaryAction({ ...base, ahead: 1 }, true)).toBe("commit");
  });
});

describe("commit message history", () => {
  test("arrowing walks older messages and returns to empty", () => {
    rememberCommitMessage("ws", "first");
    rememberCommitMessage("ws", "second");
    expect(stepCommitMessage("ws", "older")).toBe("second");
    expect(stepCommitMessage("ws", "older")).toBe("first");
    expect(stepCommitMessage("ws", "newer")).toBe("second");
    expect(stepCommitMessage("ws", "newer")).toBe("");
    resetCommitMessageCursor("ws");
  });
});
describe("discardConfirmMessage", () => {
  test("names a single untracked file as a permanent delete", () => {
    expect(discardConfirmMessage([{ path: "src/new.ts", worktreeStatus: "?" }])).toBe(
      "确定永久删除未跟踪文件 new.ts？此操作不可撤销。",
    );
  });

  test("warns that an untracked directory delete removes everything inside", () => {
    expect(discardConfirmMessage([{ path: "scratch/", worktreeStatus: "?" }])).toContain(
      "未跟踪目录 scratch",
    );
    expect(discardConfirmMessage([{ path: "scratch/", worktreeStatus: "?" }])).toContain(
      "内部所有未跟踪文件",
    );
  });

  test("does not treat a conflict as an untracked delete", () => {
    expect(discardConfirmMessage([{ path: "src/app.ts", worktreeStatus: "U" }])).toBe(
      "确定撤回 app.ts 的本地更改？未保存的修改将丢失。",
    );
  });
});

describe("publishSkipsStagedChanges", () => {
  test("empty message with staged files does not include them in publish", () => {
    expect(publishSkipsStagedChanges("  ", 2)).toBe(true);
    expect(publishWithoutCommitConfirm(2)).toContain("仍留在本地");
  });

  test("a message means publish can commit first", () => {
    expect(publishSkipsStagedChanges("feat: save", 2)).toBe(false);
  });
});
