import { ref, watch, type Ref } from "vue";
import type { Runtime } from "../domain/types";
import { probeRuntimeUsages, type RuntimeUsage } from "../services/backend";
import { readStorage, writeStorage } from "../utils/safe-storage";
import { isCustomRuntime } from "./runtime-list";

/** 与后端 runtime_usage::SUPPORTED 对齐：本地凭据可探测订阅用量的 CLI */
const USAGE_RUNTIME_IDS = new Set([
  "codex",
  "claude",
  "grok",
  "cursor",
  "opencode",
  "gemini",
  "antigravity",
]);

const USAGE_CACHE_KEY = "multara.runtime.usage.cache";

export function supportsRuntimeUsage(runtime: Runtime): boolean {
  return !isCustomRuntime(runtime) && USAGE_RUNTIME_IDS.has(runtime.id);
}

function readUsageCache(): Record<string, RuntimeUsage> {
  const raw = readStorage(USAGE_CACHE_KEY);
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    const out: Record<string, RuntimeUsage> = {};
    for (const [id, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (!USAGE_RUNTIME_IDS.has(id) || !value || typeof value !== "object") continue;
      const usage = value as RuntimeUsage;
      if (typeof usage.runtimeId !== "string" || !Array.isArray(usage.windows)) continue;
      out[id] = { ...usage, runtimeId: id };
    }
    return out;
  } catch {
    return {};
  }
}

function writeUsageCache(usages: Record<string, RuntimeUsage>) {
  writeStorage(USAGE_CACHE_KEY, JSON.stringify(usages));
}

export function useRuntimeUsages(runtimes: Ref<Runtime[]>) {
  // 同步读缓存，首屏直接有上次文案
  const usages = ref<Record<string, RuntimeUsage>>(readUsageCache());
  const loading = ref(false);

  async function refresh() {
    const ids = runtimes.value.filter(supportsRuntimeUsage).map((runtime) => runtime.id);
    // 离开运行时页时 ids 为空：只停转圈，绝不清掉内存/本地缓存
    if (!ids.length) {
      loading.value = false;
      return;
    }
    loading.value = true;
    try {
      const list = await probeRuntimeUsages(ids).catch(() => []);
      if (!list.length) return;
      const next = { ...usages.value };
      for (const item of list) next[item.runtimeId] = item;
      usages.value = next;
      writeUsageCache(next);
    } finally {
      loading.value = false;
    }
  }

  function setUsage(usage: RuntimeUsage) {
    usages.value = { ...usages.value, [usage.runtimeId]: usage };
    writeUsageCache(usages.value);
  }

  watch(runtimes, () => void refresh(), { immediate: true });

  return { usages, loading, refresh, setUsage };
}
