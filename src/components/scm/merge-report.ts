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
  const how = outcome.fastForward ? "快进" : "合并";
  const commit = outcome.commit ? `（${outcome.commit.slice(0, 7)}）` : "";
  const steps: MergeReportStep[] = [
    { label: `已将 ${outcome.sourceLabel} ${how}到 ${outcome.targetDisplay}${commit}`, done: true },
  ];
  if (pushRequested || outcome.pushed) {
    steps.push({ label: outcome.pushed ? "已推送到远程" : "未推送到远程", done: outcome.pushed });
  }
  if (outcome.localUpdated) steps.push({ label: "已更新本地分支", done: true });
  return { steps, notes: outcome.notes };
}
