import type { PendingDiffFile } from "../../stores/diff";
import type { PlanStatus } from "./plan-state";

/** 卡片默认展示的步骤数 */
export const PLAN_PREVIEW_STEPS = 3;

export const PLAN_STATUS_TONE: Record<PlanStatus, string> = {
  awaiting_approval: "wait",
  approved: "run",
  executing: "run",
  executed: "ok",
  failed: "fail",
  rejected: "muted",
  superseded: "muted",
};

function samePath(changed: string, filePath: string) {
  const normalized = changed.replaceAll("\\", "/");
  return normalized === filePath || normalized.endsWith(`/${filePath}`);
}

/** 规划轮的改动文件对上编辑快照（有基线的才能一键撤销） */
export function undoableChangedFiles(changedFiles: string[], snapshots: PendingDiffFile[]): PendingDiffFile[] {
  return snapshots.filter((file) => changedFiles.some((changed) => samePath(changed, file.filePath)));
}
