import { translate } from "../../i18n";
import {
  asRecord,
  formatLiveElapsed,
  inferToolName,
  resolvedInput,
  stringField,
  type PairedTool,
  type TimelineBlock,
} from "./timeline-display";

export type SubAgentStatus = "running" | "completed" | "failed" | "incomplete";

const SUBAGENT_STATUSES: SubAgentStatus[] = ["completed", "failed", "running", "incomplete"];

/** 超过这个层级不再增加缩进，改用 L4 这样的深度徽标 */
export const MAX_INDENTED_DEPTH = 3;

const MS_PER_SECOND = 1000;

const FAILED_LIFECYCLE = new Set(["failed", "error", "cancelled", "stopped"]);
const COMPLETED_LIFECYCLE = new Set(["completed", "success", "done"]);

function lifecycleStatus(item: PairedTool): string {
  return stringField(resolvedInput(item.lifecycle), ["status"]).toLowerCase();
}

function lifecycleUsage(item: PairedTool) {
  const usage = asRecord(resolvedInput(item.lifecycle).usage);
  const toolUses = Number(usage.toolUses ?? usage.tool_uses);
  const durationMs = Number(usage.durationMs ?? usage.duration_ms);
  return {
    toolUses: Number.isFinite(toolUses) ? toolUses : null,
    durationMs: Number.isFinite(durationMs) ? durationMs : null,
  };
}

/** running 指根运行是否还在进行；根运行结束后仍没有结果的子代理算未完成 */
export function subAgentStatus(item: PairedTool, running?: boolean): SubAgentStatus {
  if (item.result?.status === "error" || item.use.status === "error") return "failed";
  if (FAILED_LIFECYCLE.has(lifecycleStatus(item))) return "failed";
  if (item.result || COMPLETED_LIFECYCLE.has(lifecycleStatus(item))) return "completed";
  return running ? "running" : "incomplete";
}

export function subAgentType(item: PairedTool): string {
  return stringField(resolvedInput(item.use), ["subagent_type", "subagentType", "agent_type", "agentType"]);
}

export function subAgentDescription(item: PairedTool): string {
  const input = resolvedInput(item.use);
  return stringField(input, ["description", "title"])
    || stringField(input, ["prompt", "task"]).trim().split("\n")[0]
    || subAgentType(item)
    || inferToolName(item.use);
}

function pairedTool(block: TimelineBlock): PairedTool | undefined {
  const paired = block.type === "edit" || block.type === "shell" || block.type === "tool" || block.type === "subagent";
  return paired ? block.item : undefined;
}

function blockToolCount(block: TimelineBlock): number {
  if (block.type === "explore") return block.items.filter((entry) => entry.kind === "tool").length;
  const pair = pairedTool(block);
  return pair ? 1 + subAgentToolCount(pair.children) : 0;
}

/** 子代理内部（含更深层）发起的工具调用数 */
export function subAgentToolCount(blocks: TimelineBlock[] | undefined): number {
  return (blocks ?? []).reduce((sum, block) => sum + blockToolCount(block), 0);
}

/** 只有拿到结果才算耗时；运行中的耗时由底部「工作中」行展示 */
export function subAgentElapsedMs(item: PairedTool): number | null {
  const reported = lifecycleUsage(item).durationMs;
  if (reported != null) return Math.max(0, reported);
  const elapsed = Date.parse(item.result?.createdAt ?? "") - Date.parse(item.use.createdAt);
  return Number.isFinite(elapsed) ? Math.max(0, elapsed) : null;
}

export function subAgentResultText(item: PairedTool): string {
  return (item.result?.output || item.result?.content || "").trim();
}

export function subAgentFailureLine(item: PairedTool): string {
  return subAgentResultText(item).split("\n")[0] ?? "";
}

export function subAgentSummary(item: PairedTool, status: SubAgentStatus, language?: string): string {
  const elapsedMs = subAgentElapsedMs(item);
  return [
    translate("chat.subagent.title", language, { name: subAgentDescription(item) }),
    translate(`chat.subagent.status.${status}`, language),
    translate("chat.subagent.toolCount", language, { n: lifecycleUsage(item).toolUses ?? subAgentToolCount(item.children) }),
    elapsedMs == null ? "" : formatLiveElapsed(elapsedMs / MS_PER_SECOND, language),
  ].filter(Boolean).join(" · ");
}

function collectStatuses(blocks: TimelineBlock[] | undefined, running: boolean, into: SubAgentStatus[]) {
  for (const block of blocks ?? []) {
    if (block.type === "subagent") into.push(subAgentStatus(block.item, running));
    collectStatuses(pairedTool(block)?.children, running, into);
  }
  return into;
}

/** 回看时的折叠摘要，例如「3 个子代理，2 个完成，1 个失败」；没有子代理时为空 */
export function subAgentTallyLabel(blocks: TimelineBlock[], running?: boolean, language?: string): string {
  const statuses = collectStatuses(blocks, Boolean(running), []);
  if (!statuses.length) return "";
  const parts = SUBAGENT_STATUSES.flatMap((status) => {
    const n = statuses.filter((value) => value === status).length;
    return n ? [translate(`chat.subagent.tally.${status}`, language, { n })] : [];
  });
  return [translate("chat.subagent.tally.total", language, { n: statuses.length }), ...parts]
    .join(translate("chat.subagent.tally.separator", language));
}
