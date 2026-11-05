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

/** 常用列表全局共用（跨项目），多留一些，别让别的项目的分支名把当前项目的挤掉。 */
export const MERGE_RECENT_LIMIT = 20;
export const MERGE_RECENT_KEY = "multara.merge.recent";

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

/** `origin2/feature/x` → remote `origin2`；本地分支没有远程。 */
export function mergeRemoteOf(name: string, remote: boolean) {
  if (!remote) return "";
  const slash = name.indexOf("/");
  return slash > 0 ? name.slice(0, slash) : "";
}

/** 搜索时拿来比的名字：去掉远程前缀，`origin/uat` 和 `uat` 视为同一个名字。 */
function shortName(name: string, remote: boolean) {
  const owner = mergeRemoteOf(name, remote);
  return owner ? name.slice(owner.length + 1) : name;
}

/** 越小越靠前：整名相同 < 前缀 < 按 / - _ 分段命中 < 子串。 */
function matchScore(name: string, remote: boolean, needle: string) {
  const short = shortName(name, remote).toLowerCase();
  if (short === needle) return 0;
  if (short.startsWith(needle)) return 1;
  if (short.split(/[/_.-]/).some((part) => part.startsWith(needle))) return 2;
  if (short.includes(needle)) return 3;
  return name.toLowerCase().includes(needle) ? 4 : -1;
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
  if (needle) {
    // 搜索：本地和远程混排，按命中程度排；同分时常用的、本地的、名字短的靠前。
    const recentRank = new Map(input.recent.map((name, index) => [name, index]));
    const scored = [...input.locals, ...input.remotes]
      .map((name) => ({ name, remote: remoteSet.has(name), score: matchScore(name, remoteSet.has(name), needle) }))
      .filter((item) => item.score >= 0)
      .sort((a, b) =>
        a.score - b.score
        || (recentRank.get(a.name) ?? 1e9) - (recentRank.get(b.name) ?? 1e9)
        || Number(a.remote) - Number(b.remote)
        || a.name.length - b.name.length
        || a.name.localeCompare(b.name));
    return scored.length
      ? [{ label: t("scm.merge.group.results"), rows: scored.map((item) => row(item.name)) }]
      : [];
  }
  const recentNames = input.recent.filter(visible);
  const recentSet = new Set(recentNames);
  const groups: MergeBranchGroup[] = [];
  if (recentNames.length) groups.push({ label: t("scm.merge.group.recent"), rows: recentNames.map(row) });
  const locals = input.locals.filter((name) => !recentSet.has(name));
  if (locals.length) groups.push({ label: t("scm.localBranches"), rows: locals.map(row) });
  // 多个远程（origin / origin2…）各自一组，只有一个远程时沿用「远程分支」。
  const byRemote = new Map<string, string[]>();
  for (const name of input.remotes) {
    if (recentSet.has(name)) continue;
    const owner = mergeRemoteOf(name, true);
    byRemote.set(owner, [...(byRemote.get(owner) ?? []), name]);
  }
  for (const [owner, names] of byRemote) {
    const label = byRemote.size > 1 ? `${t("scm.remoteBranches")} · ${owner}` : t("scm.remoteBranches");
    groups.push({ label, rows: names.map(row) });
  }
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
