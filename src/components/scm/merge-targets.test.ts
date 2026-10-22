import { beforeEach, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import {
  MERGE_FILTER_ALL,
  MERGE_FILTER_RECENT,
  MERGE_FILTER_SELECTED,
  MERGE_PLAN_CONFLICT,
  MERGE_PLAN_FAST_FORWARD,
  MERGE_PLAN_MERGE,
  canMergePlan,
  mergeBranchGroups,
  mergePlanBadge,
  mergeQueueSummary,
  mergeResultLines,
  mergeRunOrder,
  rememberMergeTargets,
} from "./merge-targets";

beforeEach(() => {
  applyLanguageSetting("zh-CN");
});

describe("merge targets", () => {
  test("groups recent branches once and keeps the rest under local or remote", () => {
    const groups = mergeBranchGroups({
      locals: ["main", "develop", "hotfix/pay"],
      remotes: ["origin/staging"],
      recent: ["main", "develop"],
      filter: MERGE_FILTER_ALL,
      query: "",
      selected: ["main"],
    });
    expect(groups.map((group) => group.label)).toEqual(["常用 · 上次选过", "本地分支", "远程分支"]);
    expect(groups[0]?.rows.map((row) => row.name)).toEqual(["main", "develop"]);
    expect(groups[1]?.rows.map((row) => row.name)).toEqual(["hotfix/pay"]);
  });

  test("selected and recent filters only show that set", () => {
    const selected = mergeBranchGroups({
      locals: ["main", "develop"],
      remotes: [],
      recent: ["main"],
      filter: MERGE_FILTER_SELECTED,
      query: "dev",
      selected: ["main", "develop"],
    });
    expect(selected[0]?.rows.map((row) => row.name)).toEqual(["develop"]);
    const recent = mergeBranchGroups({
      locals: ["main"],
      remotes: ["origin/main"],
      recent: ["origin/main"],
      filter: MERGE_FILTER_RECENT,
      query: "",
      selected: [],
    });
    expect(recent[0]?.rows.map((row) => row.name)).toEqual(["origin/main"]);
  });

  test("conflicts run after clean targets, and recent picks stay in front", () => {
    expect(mergeRunOrder([
      { plan: MERGE_PLAN_CONFLICT },
      { plan: MERGE_PLAN_FAST_FORWARD },
      { plan: MERGE_PLAN_MERGE },
    ]).map((item) => item.plan)).toEqual([MERGE_PLAN_FAST_FORWARD, MERGE_PLAN_MERGE, MERGE_PLAN_CONFLICT]);
    expect(rememberMergeTargets(["main", "old"], ["develop", "main"])).toEqual(["develop", "main", "old"]);
    expect(canMergePlan(MERGE_PLAN_MERGE)).toBe(true);
    expect(mergePlanBadge(MERGE_PLAN_CONFLICT)).toBe("预计冲突");
  });

  test("queue and result copy follows the plan", () => {
    expect(mergeQueueSummary({
      branch: "release/2.4",
      display: "release/2.4",
      exists: true,
      hasRemoteBranch: true,
      commits: 2,
      files: 2,
      plan: MERGE_PLAN_CONFLICT,
      targetAhead: 1,
      conflictFiles: ["src/auth.ts", "src/login.vue"],
    }, false, "")).toBe("预计 2 个文件冲突：src/auth.ts、src/login.vue");
    expect(mergeResultLines({
      status: "merged",
      pushRequested: true,
      result: {
        status: "merged",
        sourceLabel: "feature/login",
        targetDisplay: "origin/main",
        commit: "a3f9c21",
        fastForward: true,
        pushed: true,
        localUpdated: true,
        notes: [],
        conflicts: [],
      },
    })[0]?.text).toBe("已快进到 a3f9c21 · 已更新本地分支 · 已推送到 origin/main");
  });
});
