<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, Plus, Sparkles, Trash2 } from "@lucide/vue";
import { PORT_TYPES, isValidPortName, type PortDef, type PortType } from "../../../domain/workflow";
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
  /** 展开并定位到这个端口 */
  openPort?: string;
  /** 开始节点勾了集成消息时，入参才能指定正文填到哪一项 */
  showPrimary?: boolean;
}>(), { mode: "input", reserved: () => [], readonly: false, openPort: "", showPrimary: false });

const emit = defineEmits<{ "update:modelValue": [value: PortDef[]] }>();

const expanded = reactive<Record<number, boolean>>({});
const nameDraft = ref<{ index: number; value: string } | null>(null);
const schemaText = reactive<Record<number, string>>({});
const schemaError = reactive<Record<number, string>>({});
const defaultText = reactive<Record<number, string>>({});
const sampleText = reactive<Record<number, string>>({});

const typeOptions = computed(() => PORT_TYPES.map((type) => ({ value: type, label: t(`workflow.port.typeName.${type}`) })));
const presenceOptions = computed(() => [
  { value: "required", label: t("workflow.port.required") },
  { value: "optional", label: t("workflow.field.optional") },
]);
const runInput = computed(() => props.mode === "workflow-input");
const allowMulti = computed(() => props.mode === "input");

function commit(next: PortDef[]) {
  emit("update:modelValue", next);
}

function patch(index: number, change: Partial<PortDef>) {
  commit(props.modelValue.map((port, i) => {
    if (i !== index) return port;
    const merged = { ...port, ...change };
    for (const key of Object.keys(change) as (keyof PortDef)[]) {
      // name 即使清空也要保留，否则 isValidPortName(undefined) 会把检查器整页打挂。
      if (key === "name") {
        merged.name = typeof change.name === "string" ? change.name : "";
        continue;
      }
      const flag = key === "required" || key === "multi" || key === "primary";
      if (merged[key] === undefined || merged[key] === "" || (flag && merged[key] === false)) delete merged[key];
    }
    return merged as PortDef;
  }));
}

function nextName() {
  const names = new Set(props.modelValue.map((port) => port.name));
  const base = props.mode === "output" || props.mode === "workflow-output"
    ? t("workflow.port.seed.result")
    : t("workflow.port.seed.value");
  if (!names.has(base)) return base;
  let index = 2;
  while (names.has(`${base}_${index}`)) index += 1;
  return `${base}_${index}`;
}

function add() {
  commit([...props.modelValue, { name: nextName(), type: "text", ...(runInput.value ? { required: true } : {}) }]);
}

function setPrimary(index: number, on: boolean) {
  commit(props.modelValue.map((port, i) => {
    const next = { ...port };
    if (on) {
      if (i === index) next.primary = true;
      else delete next.primary;
    } else if (i === index) {
      delete next.primary;
    }
    return next;
  }));
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

function shownName(port: PortDef, index: number) {
  if (nameDraft.value?.index === index) return nameDraft.value.value;
  return port.label?.trim() || port.name;
}

function nameError(port: PortDef, index: number): string {
  const name = shownName(port, index).trim();
  if (!isValidPortName(name)) return t("workflow.port.nameInvalid");
  if (props.modelValue.some((other, i) => i !== index && other.name.trim() === name)) return t("workflow.port.nameDuplicate");
  if (props.reserved.includes(name)) return t("workflow.port.nameReserved");
  return "";
}

function commitName(index: number, value: string) {
  nameDraft.value = null;
  const name = value.trim();
  const port = props.modelValue[index];
  if (!port) return;
  const shown = port.label?.trim() || port.name;
  if (!name || name === shown) return;
  patch(index, { name, label: "" });
}

watch(() => props.openPort, (name) => {
  if (!name) return;
  const index = props.modelValue.findIndex((port) => port.name === name);
  if (index >= 0) expanded[index] = true;
}, { immediate: true });

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

function defaultShown(port: PortDef) {
  if (port.default === undefined || port.default === null) return "";
  return typeof port.default === "string" ? port.default : JSON.stringify(port.default);
}

function setRunDefault(index: number, value: string) {
  const text = value.trim();
  patch(index, { default: text ? text : undefined });
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
    <div v-if="!modelValue.length" class="wf-empty">{{ t(runInput ? "workflow.field.empty" : "workflow.port.empty") }}</div>
    <div v-for="(port, index) in modelValue" :key="index" class="port-item wf-card" :data-field="mode === 'input' ? `inputs:${port.name}` : mode === 'output' ? `outputs:${port.name}` : undefined">
      <div class="wf-form">
        <div class="wf-field">
          <span class="port-name-row">
            <button v-if="!runInput" type="button" class="port-toggle" :aria-label="t('workflow.port.details')" @click.prevent="toggle(index)">
              <component :is="expanded[index] ? ChevronDown : ChevronRight" :size="14" />
            </button>
            <span>{{ t(runInput ? "workflow.field.name" : "workflow.port.name") }}</span>
            <span v-if="!readonly" class="port-actions">
              <UiButton variant="ghost" size="icon" :tip="t('workflow.port.moveUp')" :disabled="index === 0" @click="move(index, -1)"><ArrowUp :size="13" /></UiButton>
              <UiButton variant="ghost" size="icon" :tip="t('workflow.port.moveDown')" :disabled="index === modelValue.length - 1" @click="move(index, 1)"><ArrowDown :size="13" /></UiButton>
              <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(index)"><Trash2 :size="13" /></UiButton>
            </span>
          </span>
          <input
            class="ui-input wf-mono"
            :class="{ 'is-error': nameError(port, index) }"
            :value="shownName(port, index)"
            :disabled="readonly"
            spellcheck="false"
            @input="nameDraft = { index, value: ($event.target as HTMLInputElement).value }"
            @blur="commitName(index, ($event.target as HTMLInputElement).value)"
          >
          <p v-if="nameError(port, index)" class="wf-error">{{ nameError(port, index) }}</p>
        </div>
        <label v-if="runInput" class="wf-field">
          <span class="wf-label">{{ t("workflow.field.presence") }}</span>
          <UiSelect
            size="sm"
            :model-value="port.required ? 'required' : 'optional'"
            :options="presenceOptions"
            :disabled="readonly"
            @update:model-value="patch(index, { required: $event === 'required' })"
          />
        </label>
        <label v-if="runInput" class="wf-field">
          <span class="wf-label">{{ t("workflow.port.default") }}</span>
          <input
            class="ui-input"
            :value="defaultShown(port)"
            :disabled="readonly"
            :placeholder="t('workflow.port.defaultPlaceholder')"
            @change="setRunDefault(index, ($event.target as HTMLInputElement).value)"
          >
        </label>
        <label v-if="!runInput" class="wf-field">
          <span class="wf-label">{{ t("workflow.port.type") }}</span>
          <UiSelect
            size="sm"
            :model-value="port.type"
            :options="typeOptions"
            :disabled="readonly"
            @update:model-value="setType(index, $event as PortType)"
          />
        </label>
        <template v-if="expanded[index] && !runInput">
          <label v-if="mode === 'input' || mode === 'workflow-input'" class="wf-field">
            <span class="wf-label">{{ t("workflow.port.default") }}</span>
            <input
              v-model="defaultText[index]"
              data-focus
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
        </template>
      </div>
      <div class="port-checks">
        <label v-if="!runInput" class="wf-check">
          <input type="checkbox" class="ui-checkbox" :checked="Boolean(port.required)" :disabled="readonly" @change="patch(index, { required: ($event.target as HTMLInputElement).checked })">
          {{ mode === "output" ? t("workflow.port.requiredOutput") : t("workflow.port.required") }}
        </label>
        <label v-if="allowMulti" class="wf-check">
          <input type="checkbox" class="ui-checkbox" :checked="Boolean(port.multi)" :disabled="readonly" @change="patch(index, { multi: ($event.target as HTMLInputElement).checked })">
          {{ t("workflow.port.multi") }}
        </label>
        <label v-if="mode === 'workflow-input' && showPrimary" class="wf-check primary-check">
          <input type="checkbox" class="ui-checkbox" :checked="Boolean(port.primary)" :disabled="readonly" @change="setPrimary(index, ($event.target as HTMLInputElement).checked)">
          <span class="primary-copy">
            <span>{{ t("workflow.port.primary") }}</span>
            <span class="wf-hint">{{ t("workflow.port.primaryHint") }}</span>
          </span>
        </label>
      </div>
    </div>
    <UiButton v-if="!readonly" size="small" @click="add"><Plus :size="14" /> {{ t(runInput ? "workflow.field.add" : "workflow.port.add") }}</UiButton>
  </div>
</template>

<style scoped>
.port-list {
  display: grid;
  gap: var(--spacing-2);
  min-width: 0;
}
.port-item {
  gap: var(--spacing-3);
}
.port-name-row {
  display: flex;
  align-items: center;
  gap: var(--spacing-1);
  min-width: 0;
}
.port-name-row > span:first-of-type {
  min-width: 0;
}
.port-actions {
  display: inline-flex;
  flex: none;
  align-items: center;
  margin-left: auto;
}
.port-checks {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--spacing-2) var(--spacing-3);
}
.primary-check { align-items: flex-start; flex-basis: 100%; }
.primary-copy { display: grid; gap: 1px; min-width: 0; }
.primary-copy .wf-hint { margin: 0; }
.port-toggle {
  display: inline-grid;
  place-items: center;
  width: 16px;
  height: 16px;
  margin: 0;
  padding: 0;
  border: 0;
  background: none;
  color: var(--color-text-tertiary);
  cursor: pointer;
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
