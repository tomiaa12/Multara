<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Copy, Download } from "@lucide/vue";
import { t } from "../../../i18n";
import * as backend from "../../../services/backend";
import type { ValueMeta } from "../../../domain/workflow";
import MarkdownPreview from "../../workbench/MarkdownPreview.vue";

const INLINE_LIMIT = 4096;

const props = defineProps<{
  /** 已知的值（预览或完整值） */
  value?: unknown;
  type?: string;
  /** 后端的值元数据：preview 为空且 size 超过 4KB 时可按需加载完整值 */
  meta?: ValueMeta | null;
}>();

const full = ref<unknown>(undefined);
const loading = ref(false);
const error = ref("");
const copied = ref(false);

watch(() => [props.meta?.id, props.value], () => {
  full.value = undefined;
  error.value = "";
});

const shown = computed(() => (full.value !== undefined ? full.value : props.value));
const kind = computed(() => props.type ?? props.meta?.portType ?? "any");
const needsLoad = computed(() =>
  full.value === undefined
  && props.meta != null
  && (props.value === undefined || props.value === null)
  && props.meta.size > INLINE_LIMIT,
);

const text = computed(() => {
  const value = shown.value;
  if (typeof value === "string") return value;
  if (value === undefined) return "";
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
});

const isMarkdown = computed(() => kind.value === "markdown" && typeof shown.value === "string");
const isLink = computed(() => kind.value === "url" && typeof shown.value === "string");
const fileList = computed<string[]>(() => {
  if (kind.value === "file" && typeof shown.value === "string") return [shown.value];
  if (kind.value === "files" && Array.isArray(shown.value)) return shown.value.map(String);
  return [];
});

async function loadFull() {
  if (!props.meta) return;
  loading.value = true;
  error.value = "";
  try {
    const values = await backend.getWorkflowNodeValues(props.meta.nodeRunId);
    const outputs = values.outputs as unknown;
    let found: unknown;
    if (Array.isArray(outputs)) {
      const hit = outputs.find((item) => item && typeof item === "object" && (item as { port?: string }).port === props.meta!.port) as
        | { value?: unknown }
        | undefined;
      found = hit?.value;
    } else if (outputs && typeof outputs === "object") {
      found = (outputs as Record<string, unknown>)[props.meta.port];
    }
    full.value = found ?? null;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading.value = false;
  }
}

async function copy(content: string) {
  try {
    await navigator.clipboard.writeText(content);
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1200);
  } catch {
    // 剪贴板不可用
  }
}
</script>

<template>
  <div class="value-viewer">
    <div v-if="needsLoad" class="value-load">
      <span>{{ t("workflow.run.valueLarge", { size: meta?.size ?? 0 }) }}</span>
      <UiButton size="small" :disabled="loading" @click="loadFull"><Download :size="13" /> {{ t("workflow.run.valueLoad") }}</UiButton>
      <span v-if="error" class="value-error">{{ error }}</span>
    </div>
    <template v-else>
      <div v-if="shown === undefined || shown === null" class="value-empty">{{ t("workflow.run.valueNone") }}</div>
      <div v-else-if="isMarkdown" class="value-pre-wrap">
        <MarkdownPreview class="value-markdown" :content="shown as string" path="" />
        <UiButton class="value-copy" variant="ghost" size="icon" :tip="copied ? t('workflow.run.copied') : t('workflow.run.copy')" @click="copy(shown as string)"><Copy :size="13" /></UiButton>
      </div>
      <div v-else-if="isLink" class="value-pre-wrap">
        <a class="value-link" :href="shown as string" target="_blank" rel="noopener noreferrer">{{ shown }}</a>
        <UiButton class="value-copy" variant="ghost" size="icon" :tip="copied ? t('workflow.run.copied') : t('workflow.run.copy')" @click="copy(shown as string)"><Copy :size="13" /></UiButton>
      </div>
      <ul v-else-if="fileList.length" class="value-files">
        <li v-for="path in fileList" :key="path">
          <code>{{ path }}</code>
          <UiButton variant="ghost" size="icon" :tip="t('workflow.run.copy')" @click="copy(path)"><Copy :size="13" /></UiButton>
        </li>
      </ul>
      <div v-else class="value-pre-wrap">
        <pre class="value-pre">{{ text }}</pre>
        <UiButton class="value-copy" variant="ghost" size="icon" :tip="copied ? t('workflow.run.copied') : t('workflow.run.copy')" @click="copy(text)"><Copy :size="13" /></UiButton>
      </div>
    </template>
  </div>
</template>

<style scoped>
.value-viewer {
  min-width: 0;
  font-size: var(--font-size-sm);
}
.value-empty {
  color: var(--color-text-tertiary);
}
.value-load {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  color: var(--color-text-secondary);
}
.value-error {
  color: var(--color-error);
}
.value-pre-wrap {
  position: relative;
}
.value-pre-wrap :deep(.value-copy) {
  position: absolute;
  top: var(--spacing-1);
  right: var(--spacing-1);
  z-index: 1;
  opacity: 0;
  pointer-events: none;
  background: var(--color-bg-tertiary);
}
.value-pre-wrap:hover :deep(.value-copy),
.value-pre-wrap:focus-within :deep(.value-copy) {
  opacity: 1;
  pointer-events: auto;
}
.value-pre {
  margin: 0;
  max-height: 320px;
  overflow: auto;
  padding: var(--spacing-2) var(--spacing-3);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg-tertiary);
  font-family: var(--font-family-mono);
  font-size: var(--font-size-sm);
  line-height: var(--line-height-normal);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.value-link {
  color: var(--color-info);
  overflow-wrap: anywhere;
}
.value-files {
  display: grid;
  gap: var(--spacing-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
.value-files li {
  display: flex;
  align-items: center;
  gap: var(--spacing-1);
}
.value-files code {
  font-family: var(--font-family-mono);
  overflow-wrap: anywhere;
}
.value-markdown {
  position: static;
  inset: auto;
  z-index: auto;
  max-height: 240px;
  padding: var(--spacing-2) var(--spacing-3);
  overflow: auto;
  background: var(--color-bg-tertiary);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
}
.value-markdown :deep(.rich-message) {
  width: auto;
  margin: 0;
  font-size: var(--font-size-sm);
  line-height: var(--line-height-normal);
}
</style>
