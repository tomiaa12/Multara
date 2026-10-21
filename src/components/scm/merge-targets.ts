import type { MergeIntoResult, MergePreview } from "../../domain/types";

export const MERGE_FILTER_ALL = "all";
export const MERGE_FILTER_RECENT = "recent";
export const MERGE_FILTER_SELECTED = "selected";

export const MERGE_PLAN_FAST_FORWARD = "fast_forward";
export const MERGE_PLAN_MERGE = "merge";
export const MERGE_PLAN_CONFLICT = "conflict";
export const MERGE_PLAN_UP_TO_DATE = "up_to_date";
export const MERGE_PLAN_MISSING = "missing";

export const MERGE_RECENT_LIMIT = 8;

export interface MergeBranchRow {
  name: string;
  remote: boolean;
}

export interface MergeBranchGroup {
  label: string;
  rows: MergeBranchRow[];
}

type TargetPreview = NonNullable<MergePreview["target"]>;

export function mergePlanBadge(plan: string) {
  if (plan === MERGE_PLAN_FAST_FORWARD) return "可快进";
  if (plan === MERGE_PLAN_MERGE) return "合并提交";
  if (plan === MERGE_PLAN_CONFLICT) return "预计冲突";
  if (plan === MERGE_PLAN_UP_TO_DATE) return "已包含";
  if (plan === MERGE_PLAN_MISSING) return "找不到";
  return "";
}

export function mergeQueueSummary(preview: TargetPreview | null, loading: boolean, error: string) {
  if (error) return error;
  if (loading || !preview) return "正在查看…";
  if (preview.plan === MERGE_PLAN_FAST_FORWARD) {
    return `合入 ${preview.commits} 个提交 · ${preview.files} 个文件`;
  }
  if (preview.plan === MERGE_PLAN_MERGE) {
    return `将生成合并提交 · 目标领先 ${preview.targetAhead} 个提交`;
  }
  if (preview.plan === MERGE_PLAN_CONFLICT) {
    const names = preview.conflictFiles.join("、");
    if (!names) return `预计 ${preview.conflictFiles.length} 个文件冲突`;
    return `预计 ${preview.conflictFiles.length} 个文件冲突：${names}`;
  }
  if (preview.plan === MERGE_PLAN_UP_TO_DATE) return "已经包含当前分支的所有提交";
  if (preview.plan === MERGE_PLAN_MISSING) return "找不到这个分支，请先获取远程更新";
  return "正在查看…";
}

export function canMergePlan(plan: string) {
  return plan === MERGE_PLAN_FAST_FORWARD || plan === MERGE_PLAN_MERGE || plan === MERGE_PLAN_CONFLICT;
}

/** 无冲突的先做，冲突留到队列最后。 */
export function mergeRunOrder<T extends { plan: string }>(items: T[]): T[] {
  const ready = items.filter((item) =>
    item.plan === MERGE_PLAN_FAST_FORWARD || item.plan === MERGE_PLAN_MERGE || item.plan === MERGE_PLAN_UP_TO_DATE,
  );
  const conflicts = items.filter((item) => item.plan === MERGE_PLAN_CONFLICT);
  return [...ready, ...conflicts];
}

export function rememberMergeTargets(previous: string[], picked: string[]) {
  const next = [...picked, ...previous.filter((name) => !picked.includes(name))];
  return next.slice(0, MERGE_RECENT_LIMIT);
}

export function mergeBranchGroups(input: {
  locals: string[];
  remotes: string[];
  recent: string[];
  filter: string;
  query: string;
  selected: string[];
}): MergeBranchGroup[] {
  const remoteSet = new Set(input.remotes);
  const known = new Set([...input.locals, ...input.remotes]);
  const needle = input.query.trim().toLowerCase();
  const match = (name: string) => !needle || name.toLowerCase().includes(needle);
  const row = (name: string): MergeBranchRow => ({ name, remote: remoteSet.has(name) });
  const visible = (name: string) => known.has(name) && match(name);

  if (input.filter === MERGE_FILTER_SELECTED) {
    const rows = input.selected.filter(visible).map(row);
    return rows.length ? [{ label: "", rows }] : [];
  }
  if (input.filter === MERGE_FILTER_RECENT) {
    const rows = input.recent.filter(visible).map(row);
    return rows.length ? [{ label: "常用 · 上次选过", rows }] : [];
  }
  const recentNames = input.recent.filter(visible);
  const recentSet = new Set(recentNames);
  const groups: MergeBranchGroup[] = [];
  if (recentNames.length) groups.push({ label: "常用 · 上次选过", rows: recentNames.map(row) });
  const locals = input.locals.filter((name) => !recentSet.has(name) && match(name));
  const remotes = input.remotes.filter((name) => !recentSet.has(name) && match(name));
  if (locals.length) groups.push({ label: "本地分支", rows: locals.map(row) });
  if (remotes.length) groups.push({ label: "远程分支", rows: remotes.map(row) });
  return groups;
}

export interface MergeResultLine {
  text: string;
  warn: boolean;
}

export function mergeResultLines(input: {
  status: string;
  result?: MergeIntoResult | null;
  error?: string;
  pushRequested: boolean;
}): MergeResultLine[] {
  if (input.status === "error") return [{ text: input.error || "合并失败", warn: true }];
  if (input.status === "pending") return [{ text: "还没开始，先处理前面的冲突", warn: false }];
  const result = input.result;
  if (!result || result.status === "up_to_date") {
    return [{ text: "已经包含当前分支的所有提交", warn: false }];
  }
  const how = result.fastForward ? "已快进到" : "已生成合并提交";
  const commit = result.commit ? ` ${result.commit}` : "";
  const facts = [`${how}${commit}`];
  if (result.localUpdated) facts.push("已更新本地分支");
  if (input.pushRequested) {
    facts.push(result.pushed ? `已推送到 ${result.targetDisplay}` : "未推送到远程");
  }
  const lines: MergeResultLine[] = [{ text: facts.join(" · "), warn: false }];
  for (const note of result.notes) lines.push({ text: note, warn: true });
  return lines;
}
