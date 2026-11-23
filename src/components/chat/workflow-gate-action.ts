import { asGateAction } from "../../domain/workflow";

export interface QuickActionLike {
  label: string;
  prompt: string;
  action?: Record<string, unknown> | null;
}

export interface GateActionDeps {
  /** 弹出输入框收集意见；返回 null 表示取消 */
  promptComment: (action: QuickActionLike) => Promise<string | null>;
  decide: (nodeRunId: string, decision: string, comment: string, form: unknown) => Promise<void>;
  onError?: (error: unknown) => void;
}

export type GateActionResult = "not-handled" | "done" | "cancelled" | "failed";

/**
 * 执行带结构化 `action` 的快捷操作。
 * `not-handled`：不是工作流审批动作，调用方应继续走「发送 prompt」的老路径。
 */
export async function runQuickAction(action: QuickActionLike, deps: GateActionDeps): Promise<GateActionResult> {
  const gate = asGateAction(action.action);
  if (!gate) return "not-handled";
  let comment = "";
  if (gate.requireComment) {
    const answer = await deps.promptComment(action);
    if (answer === null) return "cancelled";
    comment = answer;
  }
  try {
    await deps.decide(gate.nodeRunId, gate.decision, comment, gate.form ?? null);
    return "done";
  } catch (error) {
    deps.onError?.(error);
    return "failed";
  }
}
