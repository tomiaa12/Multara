import { t } from "../../i18n";

export interface MergeReportStep {
  label: string;
  done: boolean;
}

export interface MergeReport {
  steps: MergeReportStep[];
  notes: string[];
}

interface MergeOutcomeLike {
  sourceLabel: string;
  targetDisplay: string;
  fastForward?: boolean;
  commit?: string | null;
  pushed: boolean;
  localUpdated: boolean;
  notes: string[];
}

/**
 * 把合并结果拆成打勾清单：合并 → 推送 → 更新本地分支。
 * 推送只在用户勾选了推送（或实际推送了）时列出，没推成功就显示未勾选；
 * 本地分支只在确实更新时列出，没更新的原因由后端写在 notes 里。
 */
export function mergeReport(outcome: MergeOutcomeLike, pushRequested = false): MergeReport {
  const commit = outcome.commit ? `（${outcome.commit.slice(0, 7)}）` : "";
  const vars = { source: outcome.sourceLabel, target: outcome.targetDisplay, commit };
  const stepKey = outcome.fastForward ? "scm.merge.report.stepFf" : "scm.merge.report.stepMerge";
  const steps: MergeReportStep[] = [{ label: t(stepKey, vars), done: true }];
  if (pushRequested || outcome.pushed) {
    steps.push({
      label: outcome.pushed ? t("scm.merge.report.pushed") : t("scm.merge.report.notPushed"),
      done: outcome.pushed,
    });
  }
  if (outcome.localUpdated) steps.push({ label: t("scm.merge.report.localUpdated"), done: true });
  return { steps, notes: outcome.notes };
}
