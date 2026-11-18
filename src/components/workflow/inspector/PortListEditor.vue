<script setup lang="ts">
import { computed, reactive } from "vue";
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, Plus, Sparkles, Trash2 } from "@lucide/vue";
import { PORT_COLORS, PORT_TYPES, isValidPortName, type PortDef, type PortType } from "../../../domain/workflow";
import { t } from "../../../i18n";
import { inferSchema, parseJsonText } from "./schema-infer";
import "./inspector.css";

const props = withDefaults(defineProps<{
  modelValue: PortDef[];
  /** input：节点输入（可 multi / default）；output：节点输出（required = agent 必须提交）；workflow-input：运行入参（可 primary） */
  mode?: "input" | "output" | "workflow-input" | "workflow-output";
  /** 其他不可重名的端口名（如节点的隐式端口） */
  reserved?: string[];
  readonly?: boolean;
}>(), { mode: "input", reserved: () => [], readonly: false });

const emit = defineEmits<{ "update:modelValue": [value: PortDef[]] }>();

const expanded = reactive<Record<number, boolean>>({});
const schemaText = reactive<Record<number, string>>({});
const schemaError = reactive<Record<number, string>>({});
const defaultText = reactive<Record<number, string>>({});
const sampleText = reactive<Record<number, string>>({});

const typeOptions = PORT_TYPES.map((type) => ({ value: type, label: type }));
const allowMulti = computed(() => props.mode === "input");

function commit(next: PortDef[]) {
  emit("update:modelValue", next);
}

function patch(index: number, change: Partial<PortDef>) {
  commit(props.modelValue.map((port, i) => {
    if (i !== index) return port;
    const merged = { ...port, ...change };
    for (const key of Object.keys(change) as (keyof PortDef)[]) {
      const flag = key === "required" || key === "multi" || key === "primary";
      if (merged[key] === undefined || merged[key] === "" || (flag && merged[key] === false)) delete merged[key];
    }
    return merged as PortDef;
  }));
}

function nextName() {
  const names = new Set(props.modelValue.map((port) => port.name));
  const base = props.mode === "output" || props.mode === "workflow-output" ? "result" : "value";
  if (!names.has(base)) return base;
  let index = 2;
  while (names.has(`${base}_${index}`)) index += 1;
  return `${base}_${index}`;
}

function add() {
  commit([...props.modelValue, { name: nextName(), type: "text" }]);
}

function remove(index: number) {
  commit(props.modelValue.filter((_, i) => i !== index));
}

function move(index: number, delta: number) {
  const target = index + delta;
  if (target < 0 || target >= props.modelValue.length) return;
  const next = [...props.modelValue];
  [next[index], next[target]] = [next[target], next[index]];
  commit(next);
}

function nameError(port: PortDef, index: number): string {
  if (!isValidPortName(port.name)) return t("workflow.port.nameInvalid");
  if (props.modelValue.some((other, i) => i !== index && other.name === port.name)) return t("workflow.port.nameDuplicate");
  if (props.reserved.includes(port.name)) return t("workflow.port.nameReserved");
  return "";
}

function toggle(index: number) {
  expanded[index] = !expanded[index];
  if (expanded[index]) {
    const port = props.modelValue[index];
    schemaText[index] = port.schema ? JSON.stringify(port.schema, null, 2) : "";
    defaultText[index] = port.default === undefined ? "" : JSON.stringify(port.default);
  }
}

function setType(index: number, type: PortType) {
  const change: Partial<PortDef> = { type };
  if (type !== "json") change.schema = undefined;
  patch(index, change);
}

function applySchemaText(index: number) {
  const parsed = parseJsonText(schemaText[index] ?? "");
  if (parsed instanceof Error) {
    schemaError[index] = parsed.message;
    return;
  }
  schemaError[index] = "";
  patch(index, { schema: parsed === undefined ? undefined : (parsed as Record<string, unknown>) });
}

function generateSchema(index: number) {
  const parsed = parseJsonText(sampleText[index] ?? "");
  if (parsed === undefined) return;
  if (parsed instanceof Error) {
    schemaError[index] = parsed.message;
    return;
  }
  const schema = inferSchema(parsed);
  schemaText[index] = JSON.stringify(schema, null, 2);
  schemaError[index] = "";
  patch(index, { schema });
}

function applyDefault(index: number) {
  const text = defaultText[index] ?? "";
  if (!text.trim()) return patch(index, { default: undefined });
  const parsed = parseJsonText(text);
  // 非 JSON 的文本当作字符串默认值
  patch(index, { default: parsed instanceof Error ? text : parsed });
}
</script>

<template>
  <div class="port-list">
    <div v-if="!modelValue.length" class="wf-empty">{{ t("workflow.port.empty") }}</div>
    <div v-for="(port, index) in modelValue" :key="index" class="port-item wf-card">
      <div class="wf-row">
        <button type="button" class="port-toggle" :aria-label="t('workflow.port.details')" @click="toggle(index)">
          <component :is="expanded[index] ? ChevronDown : ChevronRight" :size="14" />
        </button>
        <span class="dot" :style="{ background: PORT_COLORS[port.type] }" />
        <input
          class="ui-input wf-mono grow-1"
          :class="{ 'is-error': nameError(port, index) }"
          :value="port.name"
          :disabled="readonly"
          :placeholder="t('workflow.port.name')"
          @input="patch(index, { name: ($event.target as HTMLInputElement).value })"
        >
        <UiSelect
          class="type-select"
          size="sm"
          :model-value="port.type"
          :options="typeOptions"
          :disabled="readonly"
          @update:model-value="setType(index, $event as PortType)"
        />
        <UiButton v-if="!readonly" variant="ghost" size="icon" :tip="t('workflow.port.moveUp')" :disabled="index === 0" @click="move(index, -1)"><ArrowUp :size="13" /></UiButton>
        <UiButton v-if="!readonly" variant="ghost" size="icon" :tip="t('workflow.port.moveDown')" :disabled="index === modelValue.length - 1" @click="move(index, 1)"><ArrowDown :size="13" /></UiButton>
        <UiButton v-if="!readonly" variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(index)"><Trash2 :size="13" /></UiButton>
      </div>
      <p v-if="nameError(port, index)" class="wf-error">{{ nameError(port, index) }}</p>
      <div class="wf-row">
        <label class="wf-check">
          <input type="checkbox" class="ui-checkbox" :checked="Boolean(port.required)" :disabled="readonly" @change="patch(index, { required: ($event.target as HTMLInputElement).checked })">
          {{ mode === "output" ? t("workflow.port.requiredOutput") : t("workflow.port.required") }}
        </label>
        <label v-if="allowMulti" class="wf-check">
          <input type="checkbox" class="ui-checkbox" :checked="Boolean(port.multi)" :disabled="readonly" @change="patch(index, { multi: ($event.target as HTMLInputElement).checked })">
          {{ t("workflow.port.multi") }}
        </label>
        <label v-if="mode === 'workflow-input'" class="wf-check">
          <input type="checkbox" class="ui-checkbox" :checked="Boolean(port.primary)" :disabled="readonly" @change="patch(index, { primary: ($event.target as HTMLInputElement).checked })">
          {{ t("workflow.port.primary") }}
        </label>
      </div>
      <div v-if="expanded[index]" class="wf-form port-details">
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.port.label") }}</span>
          <UiInput :model-value="port.label ?? ''" :disabled="readonly" @update:model-value="patch(index, { label: String($event) })" />
        </label>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.port.description") }}</span>
          <UiTextarea class="wf-textarea" :model-value="port.description ?? ''" :disabled="readonly" @update:model-value="patch(index, { description: $event })" />
          <span v-if="mode === 'output'" class="wf-hint">{{ t("workflow.port.descriptionHint") }}</span>
        </label>
        <label v-if="mode === 'input' || mode === 'workflow-input'" class="wf-field">
          <span class="wf-label">{{ t("workflow.port.default") }}</span>
          <input
            v-model="defaultText[index]"
            class="ui-input wf-mono"
            :disabled="readonly"
            :placeholder="t('workflow.port.defaultPlaceholder')"
            @blur="applyDefault(index)"
          >
        </label>
        <div v-if="port.type === 'json'" class="wf-field">
          <span class="wf-label">{{ t("workflow.port.schema") }}</span>
          <textarea
            v-model="schemaText[index]"
            class="ui-textarea wf-textarea"
            rows="6"
            spellcheck="false"
            :disabled="readonly"
            :placeholder="t('workflow.port.schemaPlaceholder')"
            @blur="applySchemaText(index)"
          />
          <p v-if="schemaError[index]" class="wf-error">{{ schemaError[index] }}</p>
          <details v-if="!readonly" class="sample">
            <summary><Sparkles :size="12" /> {{ t("workflow.port.fromSample") }}</summary>
            <textarea v-model="sampleText[index]" class="ui-textarea wf-textarea" rows="4" spellcheck="false" :placeholder="t('workflow.port.samplePlaceholder')" />
            <UiButton size="small" @click="generateSchema(index)">{{ t("workflow.port.generate") }}</UiButton>
          </details>
        </div>
      </div>
    </div>
    <UiButton v-if="!readonly" size="small" @click="add"><Plus :size="14" /> {{ t("workflow.port.add") }}</UiButton>
  </div>
</template>

<style scoped>
.port-list {
  display: grid;
  gap: var(--spacing-2);
  min-width: 0;
}
.port-toggle {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: 0;
  background: none;
  color: var(--color-text-tertiary);
  cursor: pointer;
}
.dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: var(--radius-full);
}
.type-select {
  width: 96px;
  flex: none;
}
.port-details {
  padding-top: var(--spacing-2);
  border-top: 1px solid var(--color-border-light);
}
.sample {
  display: grid;
  gap: var(--spacing-1-5);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.sample summary {
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: var(--spacing-1);
}
</style>
