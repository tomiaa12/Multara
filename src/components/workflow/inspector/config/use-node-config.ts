import { computed } from "vue";
import { useWorkflowsStore } from "../../../../stores/workflows";
import { useAppStore } from "../../../../stores/app";
import type { NodeDef, WorkflowDef } from "../../../../domain/workflow";
import { renameNodeInput, renameNodeOutput } from "../../../../domain/workflow-rename";
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

/** 同一位置只改了一个端口名时返回这一对；增删或调换顺序不算改名。 */
export function renamedPort(before: { name: string }[], after: { name: string }[]): { from: string; to: string } | null {
  if (before.length !== after.length) return null;
  let found: { from: string; to: string } | null = null;
  for (let index = 0; index < before.length; index += 1) {
    if (before[index].name === after[index].name) continue;
    if (found) return null;
    found = { from: before[index].name, to: after[index].name };
  }
  return found;
}

/** 输出端口改名：连线保留，模板里的 nodes.<id>.outputs.旧名 一起换掉。 */
export function renameSourcePort(def: WorkflowDef, nodeId: string, from: string, to: string) {
  renameNodeOutput(def, nodeId, from, to);
}

/** 输入端口改名：连线保留，本节点模板里的 inputs.旧名 一起换掉。 */
export function renameTargetPort(def: WorkflowDef, nodeId: string, from: string, to: string) {
  renameNodeInput(def, nodeId, from, to);
}

export function parseNumberList(text: string): number[] {
  return text
    .split(/[,\s]+/)
    .map((item) => item.trim())
    .filter(Boolean)
    .map(Number)
    .filter((value) => Number.isFinite(value));
}
