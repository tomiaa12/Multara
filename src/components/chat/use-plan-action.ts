import { ref } from "vue";
import type { PlanActionKind } from "../../domain/types";
import { useAppStore } from "../../stores/app";
import { useI18n } from "../../i18n";
import type { PlanEntry } from "./plan-state";

export function usePlanAction(sessionId: () => string) {
  const store = useAppStore();
  const { t } = useI18n();
  const pendingAction = ref<PlanActionKind | null>(null);
  const actionError = ref("");

  /** 成功返回 true；后端按 planId + revision + action 幂等，重复点击不会重复执行 */
  async function act(entry: PlanEntry | null | undefined, action: PlanActionKind) {
    if (!entry || pendingAction.value) return false;
    pendingAction.value = action;
    actionError.value = "";
    try {
      await store.planAction({
        sessionId: sessionId(),
        planRunId: entry.runId ?? "",
        planId: entry.artifact.planId,
        revision: entry.artifact.revision,
        action,
      });
      return true;
    } catch (cause) {
      actionError.value = t("chat.plan.actionFailed", { message: cause instanceof Error ? cause.message : String(cause) });
      return false;
    } finally {
      pendingAction.value = null;
    }
  }

  return { pendingAction, actionError, act };
}
