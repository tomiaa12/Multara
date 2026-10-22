import type { MergeIntoResult, MergePreview } from "../../domain/types";
import { t } from "../../i18n";

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
  if (plan === MERGE_PLAN_FAST_FORWARD) return t("scm.merge.plan.fastForward");
  if (plan === MERGE_PLAN_MERGE) return t("scm.merge.plan.mergeCommit");
  if (plan === MERGE_PLAN_CONFLICT) return t("scm.merge.plan.conflict");
  if (plan === MERGE_PLAN_UP_TO_DATE) return t("scm.merge.plan.upToDate");
  if (plan === MERGE_PLAN_MISSING) return t("scm.merge.plan.missing");
  return "";
}

export function mergeQueueSummary(preview: TargetPreview | null, loading: boolean, error: string) {
  if (error) return error;
  if (loading || !preview) return t("scm.merge.queue.loading");
  if (preview.plan === MERGE_PLAN_FAST_FORWARD) {
    return t("scm.merge.queue.ffSummary", { commits: preview.commits, files: preview.files });
  }
  if (preview.plan === MERGE_PLAN_MERGE) {
    return t("scm.merge.queue.mergeSummary", { ahead: preview.targetAhead });
  }
  if (preview.plan === MERGE_PLAN_CONFLICT) {
    const names = preview.conflictFiles.join(t("common.listSep"));
    if (!names) return t("scm.merge.queue.conflicts", { count: preview.conflictFiles.length });
    return t("scm.merge.queue.conflictsNamed", { count: preview.conflictFiles.length, names });
  }
  if (preview.plan === MERGE_PLAN_UP_TO_DATE) return t("scm.merge.queue.upToDate");
  if (preview.plan === MERGE_PLAN_MISSING) return t("scm.merge.queue.missing");
  return t("scm.merge.queue.loading");
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
    return rows.length ? [{ label: t("scm.merge.group.recent"), rows }] : [];
  }
  const recentNames = input.recent.filter(visible);
  const recentSet = new Set(recentNames);
  const groups: MergeBranchGroup[] = [];
  if (recentNames.length) groups.push({ label: t("scm.merge.group.recent"), rows: recentNames.map(row) });
  const locals = input.locals.filter((name) => !recentSet.has(name) && match(name));
  const remotes = input.remotes.filter((name) => !recentSet.has(name) && match(name));
  if (locals.length) groups.push({ label: t("scm.localBranches"), rows: locals.map(row) });
  if (remotes.length) groups.push({ label: t("scm.remoteBranches"), rows: remotes.map(row) });
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
  if (input.status === "error") return [{ text: input.error || t("scm.merge.result.failed"), warn: true }];
  if (input.status === "pending") return [{ text: t("scm.merge.result.pending"), warn: false }];
  const result = input.result;
  if (!result || result.status === "up_to_date") {
    return [{ text: t("scm.merge.result.upToDate"), warn: false }];
  }
  const how = result.fastForward ? t("scm.merge.result.fastForward") : t("scm.merge.result.mergeCommit");
  const commit = result.commit ? ` ${result.commit}` : "";
  const facts = [`${how}${commit}`];
  if (result.localUpdated) facts.push(t("scm.merge.result.localUpdated"));
  if (input.pushRequested) {
    facts.push(result.pushed ? t("scm.merge.result.pushedTo", { target: result.targetDisplay }) : t("scm.merge.result.notPushed"));
  }
  const lines: MergeResultLine[] = [{ text: facts.join(" · "), warn: false }];
  for (const note of result.notes) lines.push({ text: note, warn: true });
  return lines;
}
