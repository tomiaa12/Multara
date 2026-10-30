import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import type { GlobalResource, McpProbe, McpToolsCacheEntry } from "../domain/types";
import * as backend from "../services/backend";

/** 统一 MCP 工具缓存：读磁盘/内存缓存，后台串行刷新，不阻塞 UI。 */
export const useMcpToolsStore = defineStore("mcp-tools", () => {
  const probes = ref<Record<string, McpProbe>>({});
  const loadingIds = ref<Record<string, true>>({});
  const ready = ref(false);
  let unlisten: UnlistenFn | null = null;
  let listening: Promise<void> | null = null;

  const loading = computed(() => loadingIds.value);

  /** 每个服务器一条事件，原地更新这一项，不克隆整张映射（含日志）。 */
  function applyEntry(entry: McpToolsCacheEntry) {
    probes.value[entry.id] = entry.probe;
    delete loadingIds.value[entry.id];
  }

  async function ensureListener() {
    if (listening) return listening;
    listening = (async () => {
      if (unlisten) return;
      unlisten = await listen<McpToolsCacheEntry>("mcp-tools-cache-updated", (event) => {
        if (event.payload?.id) applyEntry(event.payload);
      });
    })();
    return listening;
  }

  async function hydrate() {
    await ensureListener();
    try {
      const rows = await backend.listMcpToolCache();
      const next: Record<string, McpProbe> = { ...probes.value };
      for (const row of rows ?? []) {
        if (row?.id && row.probe) next[row.id] = row.probe;
      }
      probes.value = next;
    } finally {
      ready.value = true;
    }
  }

  /** 后台排队刷新；立即返回，不 await 探测完成。 */
  function refreshInBackground(servers: GlobalResource[], force = false) {
    const ids = (servers ?? [])
      .filter((item) => item?.enabled && item.kind === "mcp")
      .map((item) => item.id)
      .filter(Boolean);
    if (!ids.length) return;
    void ensureListener();
    const next = { ...loadingIds.value };
    for (const id of ids) {
      if (!probes.value[id] || force) next[id] = true;
    }
    loadingIds.value = next;
    void backend.scheduleMcpToolsRefresh(ids, force).catch(() => {
      loadingIds.value = {};
    });
  }

  async function probeOne(item: GlobalResource): Promise<McpProbe | null> {
    if (!item?.enabled) return null;
    loadingIds.value = { ...loadingIds.value, [item.id]: true };
    try {
      const result = await backend.probeMcp(item.name, item.owner);
      probes.value = { ...probes.value, [item.id]: result };
      return result;
    } finally {
      const next = { ...loadingIds.value };
      delete next[item.id];
      loadingIds.value = next;
    }
  }

  function isLoading(id: string) {
    return Boolean(loadingIds.value[id]);
  }

  return {
    probes,
    loading,
    ready,
    hydrate,
    refreshInBackground,
    probeOne,
    isLoading,
  };
});
