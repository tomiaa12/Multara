import { computed } from "vue";
import { useWorkflowsStore } from "../../../../stores/workflows";
import { useAppStore } from "../../../../stores/app";
import type { NodeDef, WorkflowDef } from "../../../../domain/workflow";
import type { SelectOption } from "../../../ui/select-types";

/** 各配置表单共用：读写节点 config、槽位选项、改端口名时同步连线 */
export function useNodeConfig<C extends object>(getNode: () => NodeDef) {
  const store = useWorkflowsStore();
  const app = useAppStore();
  const cfg = computed(() => getNode().config as unknown as C);

  /** 修改 config 的一个或多个字段（空串 / undefined 会删除该键以保持定义精简，false/0 保留） */
  function set(patch: Partial<Record<keyof C, unknown>>, coalesce?: string) {
    const id = getNode().id;
    store.updateNode(id, (node) => {
      const target = node.config as Record<string, unknown>;
      for (const [key, value] of Object.entries(patch)) {
        if (value === undefined || value === "") delete target[key];
        else target[key] = value;
      }
    }, coalesce ?? `cfg:${id}:${Object.keys(patch).join(",")}`);
  }

  function update(change: (config: C, node: NodeDef, def: WorkflowDef) => void, coalesce?: string) {
    const id = getNode().id;
    store.updateNode(id, (node, def) => change(node.config as unknown as C, node, def), coalesce ?? `cfg:${id}`);
  }

  const slotOptions = computed<SelectOption<string>[]>(() =>
    (store.draft?.workspaces ?? []).map((slot) => ({ value: slot.key, label: slot.label ? `${slot.key} · ${slot.label}` : slot.key })),
  );

  return { store, app, cfg, set, update, slotOptions };
}

/** 分支端口改名时，把指向该端口的连线一起改掉 */
export function renameSourcePort(def: WorkflowDef, nodeId: string, from: string, to: string) {
  if (!from || from === to) return;
  for (const edge of def.edges) if (edge.source === nodeId && edge.sourcePort === from) edge.sourcePort = to;
}

export function parseNumberList(text: string): number[] {
  return text
    .split(/[,\s]+/)
    .map((item) => item.trim())
    .filter(Boolean)
    .map(Number)
    .filter((value) => Number.isFinite(value));
}
