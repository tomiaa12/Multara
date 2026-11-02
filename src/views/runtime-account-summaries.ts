import { ref, watch, type Ref } from "vue";
import type { Runtime } from "../domain/types";
import { listRuntimeAccountSummaries, type RuntimeAccountStatus } from "../services/backend";
import { isCustomRuntime } from "./runtime-list";

/** 只有检测到的官方 CLI 才有账号可管 */
export function hasAccounts(runtime: Runtime): boolean {
  return !isCustomRuntime(runtime) && runtime.status === "online";
}

export function useRuntimeAccountSummaries(runtimes: Ref<Runtime[]>) {
  const summaries = ref<Record<string, RuntimeAccountStatus>>({});

  async function refresh() {
    const ids = runtimes.value.filter(hasAccounts).map((runtime) => runtime.id);
    if (!ids.length) return;
    const list = await listRuntimeAccountSummaries(ids).catch(() => []);
    summaries.value = Object.fromEntries(list.map((item) => [item.runtimeId, item]));
  }

  function setStatus(status: RuntimeAccountStatus) {
    summaries.value = { ...summaries.value, [status.runtimeId]: status };
  }

  // 「重新检测」会换掉整个运行时数组，跟着重读账号
  watch(runtimes, () => void refresh(), { immediate: true });

  return { summaries, refresh, setStatus };
}
