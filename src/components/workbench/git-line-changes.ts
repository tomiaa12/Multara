import { applyKeepHunk, applyReversePatch } from "../../utils/diff-engine";
import type { DiffHunk } from "../../domain/diff";
import type { GitChange } from "./git-gutter-marks";

/** 把 GitChange 收成 DiffHunk，供 keep / reverse 复用现有行级补丁。 */
export function gitChangeToHunk(change: GitChange): DiffHunk {
  return {
    oldStart: change.oldStart,
    oldLines: change.oldLines,
    newStart: change.newStart,
    newLines: change.newLines,
  };
}

/**
 * 同 VS Code `applyLineChanges`：从 baseline 只叠上 keep 里的 hunks。
 * 多 hunk 从后往前 apply，避免行号偏移。
 */
export function applyLineChanges(baseline: string, keep: readonly GitChange[]): string {
  let text = baseline;
  for (const change of [...keep].sort((left, right) => right.oldStart - left.oldStart)) {
    text = applyKeepHunk(text, gitChangeToHunk(change));
  }
  return text;
}

/** 撤回单个 hunk：从当前工作区内容里去掉这块改动。 */
export function revertLineChange(current: string, change: GitChange): string {
  return applyReversePatch(current, gitChangeToHunk(change));
}
