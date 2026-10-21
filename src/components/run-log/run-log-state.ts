import { shallowRef } from "vue";
import type { TimelineItem } from "../../domain/types";

export interface OpenRunLogOptions {
  runId: string;
  status: string;
  createdAt: string;
  endedAt?: string | null;
  elapsedMs?: number | null;
  agentName?: string;
  error?: string | null;
  /** 该次运行的时间线事件；不要传假数据。 */
  timeline: TimelineItem[];
  title?: string;
}

export const runLogPayload = shallowRef<OpenRunLogOptions | null>(null);

/** 打开公共执行日志弹窗，供任务详情等页面调用。 */
export function openRunLog(options: OpenRunLogOptions): void {
  runLogPayload.value = {
    ...options,
    timeline: [...options.timeline],
  };
}

export function closeRunLog(): void {
  runLogPayload.value = null;
}
