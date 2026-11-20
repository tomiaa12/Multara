import { computed, onMounted, ref, watch, type Ref } from "vue";
import { getWorkflow } from "../../../../services/backend";
import { useWorkflowsStore } from "../../../../stores/workflows";
import type { PortDef, VersionMeta } from "../../../../domain/workflow";

/**
 * Subflow / Map 共用：子工作流选择与端口同步。
 * 契约缺口：后端没有「读已发布版本定义」的命令，这里用子工作流草稿的 inputs/outputs 近似。
 */
export function useChildWorkflow(workflowId: Ref<string>) {
  const store = useWorkflowsStore();
  const inputs = ref<PortDef[]>([]);
  const outputs = ref<PortDef[]>([]);
  const versions = ref<VersionMeta[]>([]);
  const loading = ref(false);
  const error = ref("");

  onMounted(() => {
    if (!store.listLoaded) void store.loadList().catch(() => undefined);
  });

  const options = computed(() =>
    store.list
      .filter((item) => item.publishedVersion != null && item.id !== store.workflowId)
      .map((item) => ({ value: item.id, label: item.name, meta: `v${item.publishedVersion}` })),
  );

  async function load(id: string) {
    inputs.value = [];
    outputs.value = [];
    versions.value = [];
    error.value = "";
    if (!id) return;
    loading.value = true;
    try {
      const detail = await getWorkflow(id);
      const definition = detail.published ?? detail.draft;
      inputs.value = definition.inputs;
      outputs.value = definition.outputs;
      versions.value = detail.versions;
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading.value = false;
    }
  }

  watch(workflowId, (id) => void load(id), { immediate: true });

  return { options, inputs, outputs, versions, loading, error, reload: () => load(workflowId.value) };
}
