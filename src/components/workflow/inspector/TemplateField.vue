<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { workflowRenderPreview } from "../../../services/backend";
import { useWorkflowsStore } from "../../../stores/workflows";
import { t } from "../../../i18n";
import {
  applyCompletion,
  filterCompletions,
  flattenVariables,
  tokenAtCursor,
  type CompletionItem,
  type CompletionToken,
} from "./template-completion";
import "./inspector.css";

const props = withDefaults(defineProps<{
  modelValue: string;
  nodeId: string;
  multiline?: boolean;
  /** 表达式模式：整段文本都是表达式（switch / transform），无需 {{ }} */
  expression?: boolean;
  placeholder?: string;
  rows?: number;
  /** 不显示渲染预览 */
  noPreview?: boolean;
}>(), { multiline: false, expression: false, placeholder: "", rows: 4, noPreview: false });

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const store = useWorkflowsStore();
const el = ref<HTMLTextAreaElement | HTMLInputElement | null>(null);
const items = ref<CompletionItem[]>([]);
const preview = ref<{ rendered: string | null; error: string | null } | null>(null);
const open = ref(false);
const token = ref<CompletionToken | null>(null);
const active = ref(0);
let variablesLoaded = false;
let previewTimer: ReturnType<typeof setTimeout> | null = null;
let seq = 0;

const matches = computed(() => (token.value ? filterCompletions(items.value, token.value.query) : []));

function plainDraft() {
  return store.draft ? JSON.parse(JSON.stringify(store.draft)) : null;
}

async function refresh(includePreview: boolean) {
  const draft = plainDraft();
  if (!draft || !props.nodeId) return;
  const mine = ++seq;
  try {
    const result = await workflowRenderPreview({
      draft,
      nodeId: props.nodeId,
      source: props.modelValue || (props.expression ? "null" : ""),
      isExpression: props.expression,
    });
    if (mine !== seq) return;
    items.value = flattenVariables(result.variables);
    variablesLoaded = true;
    if (includePreview) preview.value = { rendered: result.rendered, error: result.error };
  } catch (cause) {
    if (mine !== seq) return;
    if (includePreview) preview.value = { rendered: null, error: cause instanceof Error ? cause.message : String(cause) };
  }
}

function schedulePreview() {
  if (props.noPreview) return;
  if (previewTimer) clearTimeout(previewTimer);
  previewTimer = setTimeout(() => void refresh(true), 400);
}

watch(() => props.modelValue, schedulePreview, { immediate: true });
onBeforeUnmount(() => {
  if (previewTimer) clearTimeout(previewTimer);
  seq += 1;
});

const showPreview = computed(() => {
  if (props.noPreview || !preview.value) return false;
  if (preview.value.error) return true;
  if (!props.modelValue) return false;
  // 纯文本（无模板语法）时预览没有信息量
  return props.expression || /\{\{|\{%/.test(props.modelValue);
});

function onInput(event: Event) {
  const target = event.target as HTMLTextAreaElement | HTMLInputElement;
  emit("update:modelValue", target.value);
  void nextTick(() => updateCompletion(false));
}

async function updateCompletion(force: boolean) {
  const target = el.value;
  if (!target) return;
  const cursor = target.selectionStart ?? target.value.length;
  const found = tokenAtCursor(target.value, cursor, props.expression);
  if (!found && !force) {
    open.value = false;
    return;
  }
  if (!found) return;
  token.value = found;
  if (!variablesLoaded) await refresh(false);
  active.value = 0;
  open.value = matches.value.length > 0;
}

function onKeydown(event: KeyboardEvent) {
  if (event.ctrlKey && event.code === "Space") {
    event.preventDefault();
    void updateCompletion(true);
    return;
  }
  if (!open.value) return;
  if (event.key === "ArrowDown") {
    event.preventDefault();
    active.value = (active.value + 1) % matches.value.length;
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    active.value = (active.value - 1 + matches.value.length) % matches.value.length;
  } else if (event.key === "Enter" || event.key === "Tab") {
    event.preventDefault();
    pick(matches.value[active.value]);
  } else if (event.key === "Escape") {
    event.stopPropagation();
    open.value = false;
  }
}

function pick(item: CompletionItem | undefined) {
  if (!item || !token.value || !el.value) return;
  const result = applyCompletion(el.value.value, token.value, item.path, props.expression);
  emit("update:modelValue", result.text);
  open.value = false;
  void nextTick(() => {
    el.value?.focus();
    el.value?.setSelectionRange(result.cursor, result.cursor);
  });
}

function onBlur() {
  // 延后关闭，让点击候选项的 mousedown 先生效
  setTimeout(() => { open.value = false; }, 120);
}
</script>

<template>
  <div class="template-field">
    <textarea
      v-if="multiline"
      ref="el"
      class="ui-textarea wf-textarea"
      :rows="rows"
      :value="modelValue"
      :placeholder="placeholder"
      spellcheck="false"
      @input="onInput"
      @keydown="onKeydown"
      @blur="onBlur"
      @click="updateCompletion(false)"
    />
    <input
      v-else
      ref="el"
      class="ui-input wf-mono"
      type="text"
      :value="modelValue"
      :placeholder="placeholder"
      spellcheck="false"
      @input="onInput"
      @keydown="onKeydown"
      @blur="onBlur"
    >
    <ul v-if="open" class="template-popup" role="listbox">
      <li
        v-for="(item, index) in matches"
        :key="item.path"
        role="option"
        :aria-selected="index === active"
        :class="{ active: index === active }"
        @mousedown.prevent="pick(item)"
        @mouseenter="active = index"
      >
        <span class="wf-mono path">{{ item.path }}</span>
        <em>{{ item.kind }}</em>
      </li>
    </ul>
    <div v-if="showPreview" class="template-preview" :class="{ 'is-error': preview?.error }">
      <span class="tag">{{ preview?.error ? t("workflow.template.error") : t("workflow.template.preview") }}</span>
      <pre>{{ preview?.error ?? preview?.rendered }}</pre>
    </div>
  </div>
</template>

<style scoped>
.template-field {
  position: relative;
  display: grid;
  gap: var(--spacing-1);
  min-width: 0;
}
.template-popup {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  z-index: var(--z-dropdown);
  margin: var(--spacing-0-5) 0 0;
  padding: var(--spacing-1);
  max-height: 220px;
  overflow: auto;
  list-style: none;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface-raised);
  box-shadow: var(--shadow-md);
}
.template-popup li {
  display: flex;
  justify-content: space-between;
  gap: var(--spacing-2);
  padding: var(--spacing-1) var(--spacing-2);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: var(--font-size-sm);
}
.template-popup li.active {
  background: var(--color-surface-hover);
}
.template-popup .path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.template-popup em {
  flex: none;
  font-style: normal;
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
}
.template-preview {
  display: grid;
  gap: var(--spacing-0-5);
  padding: var(--spacing-1-5) var(--spacing-2);
  border-radius: var(--radius-sm);
  background: var(--color-bg-tertiary);
  min-width: 0;
}
.template-preview .tag {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
}
.template-preview pre {
  margin: 0;
  max-height: 120px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-family: var(--font-family-mono);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.template-preview.is-error pre {
  color: var(--color-error);
}
</style>
