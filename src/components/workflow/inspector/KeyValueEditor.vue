<script setup lang="ts">
import { Plus, Trash2 } from "@lucide/vue";
import { t } from "../../../i18n";
import TemplateField from "./TemplateField.vue";
import "./inspector.css";

const props = withDefaults(defineProps<{
  modelValue: Record<string, string>;
  nodeId: string;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  /** 固定的键（只编辑值），如子流程入参 */
  fixedKeys?: string[];
}>(), { keyPlaceholder: "", valuePlaceholder: "", fixedKeys: undefined });

const emit = defineEmits<{ "update:modelValue": [value: Record<string, string>] }>();

function entries(): [string, string][] {
  if (props.fixedKeys) return props.fixedKeys.map((key) => [key, props.modelValue[key] ?? ""]);
  return Object.entries(props.modelValue);
}

function commit(next: [string, string][]) {
  const out: Record<string, string> = {};
  for (const [key, value] of next) {
    if (props.fixedKeys && value === "") continue;
    out[key] = value;
  }
  emit("update:modelValue", out);
}

function setValue(index: number, value: string) {
  const next = entries();
  next[index] = [next[index][0], value];
  commit(next);
}

function setKey(index: number, key: string) {
  const next = entries();
  next[index] = [key, next[index][1]];
  commit(next);
}

function add() {
  const next = entries();
  let name = "KEY";
  let index = 2;
  while (next.some(([key]) => key === name)) name = `KEY_${index++}`;
  next.push([name, ""]);
  commit(next);
}

function remove(index: number) {
  const next = entries();
  next.splice(index, 1);
  commit(next);
}
</script>

<template>
  <div class="kv">
    <div v-for="([key, value], index) in entries()" :key="fixedKeys ? key : index" class="kv-row">
      <span v-if="fixedKeys" class="wf-mono kv-fixed">{{ key }}</span>
      <input
        v-else
        class="ui-input wf-mono kv-key"
        :value="key"
        :placeholder="keyPlaceholder"
        @change="setKey(index, ($event.target as HTMLInputElement).value)"
      >
      <TemplateField class="kv-value" :model-value="value" :node-id="nodeId" :placeholder="valuePlaceholder" no-preview @update:model-value="setValue(index, $event)" />
      <UiButton v-if="!fixedKeys" variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(index)"><Trash2 :size="13" /></UiButton>
    </div>
    <div v-if="!fixedKeys"><UiButton size="small" @click="add"><Plus :size="14" /> {{ t("workflow.config.add") }}</UiButton></div>
  </div>
</template>

<style scoped>
.kv {
  display: grid;
  gap: var(--spacing-1-5);
}
.kv-row {
  display: flex;
  align-items: flex-start;
  gap: var(--spacing-1-5);
  min-width: 0;
}
.kv-key {
  width: 38%;
  flex: none;
}
.kv-fixed {
  width: 34%;
  flex: none;
  padding-top: var(--spacing-1-5);
  overflow: hidden;
  text-overflow: ellipsis;
}
.kv-value {
  flex: 1;
}
</style>
