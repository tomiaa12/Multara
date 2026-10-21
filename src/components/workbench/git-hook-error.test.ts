// @vitest-environment node
import { describe, expect, test } from "vitest";
import { parseGitHookFailure } from "./git-hook-error";

describe("parseGitHookFailure", () => {
  test("reads a local hook diagnostic and ignores the prefix", () => {
    const raw = [
      '@@multara-hook@@{"kind":"local_hook","repairable":true,"skippable":true,"operation":"push"}',
      "推送被本地 `pre-push` 钩子拦截（不是远程拒绝）：.husky/pre-push: line 2: .husky/_/husky.sh: No such file or directory",
    ].join("\n");
    expect(parseGitHookFailure(new Error(raw))).toEqual({
      kind: "local_hook",
      repairable: true,
      skippable: true,
      operation: "push",
      message: "推送被本地 `pre-push` 钩子拦截（不是远程拒绝）：.husky/pre-push: line 2: .husky/_/husky.sh: No such file or directory",
    });
  });

  test("leaves ordinary git errors alone", () => {
    expect(parseGitHookFailure(new Error("远程拒绝了这次推送：non-fast-forward"))).toBeNull();
  });

  test("does not offer skip when the payload says the hook cannot be bypassed", () => {
    const raw = '@@multara-hook@@{"kind":"deps_mismatch","repairable":false,"skippable":false,"operation":"sync"}\n该分支依赖与主检出不同';
    expect(parseGitHookFailure(raw)?.skippable).toBe(false);
  });
});
