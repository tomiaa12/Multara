<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { Play, X } from "@lucide/vue";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import type { PortDef, WorkflowDef, WorkflowRun } from "../../domain/workflow";

const props = defineProps<{
  workflowId: string;
  definition: Pick<WorkflowDef, "inputs">;
  /** true：用草稿试运行（version 0） */
  draft?: boolean;
}>();
const emit = defineEmits<{ close: []; started: [run: WorkflowRun] }>();

const STORAGE_KEY = `multara.workflow.lastInputs.${props.workflowId}`;

function readSaved(): Record<string, unknown> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

function toText(port: PortDef, value: unknown): string {
  if (value === undefined || value === null) return "";
  if (port.type === "json" || port.type === "any") return typeof value === "string" ? value : JSON.stringify(value, null, 2);
  if (port.type === "files") return Array.isArray(value) ? value.join("\n") : String(value);
  return String(value);
}

const saved = readSaved();
const ports = computed(() => props.definition.inputs);
const values = reactive<Record<string, string>>({});
const flags = reactive<Record<string, boolean>>({});
for (const port of props.definition.inputs) {
  const initial = port.name in saved ? saved[port.name] : port.default;
  if (port.type === "boolean") flags[port.name] = Boolean(initial);
  else values[port.name] = toText(port, initial);
}
const errors = reactive<Record<string, string>>({});
const busy = ref(false);
const error = ref("");

function isMultiline(port: PortDef) {
  return ["markdown", "json", "any", "files"].includes(port.type);
}

function label(port: PortDef) {
  return port.label || port.name;
}

function build(): Record<string, unknown> | null {
  let ok = true;
  const result: Record<string, unknown> = {};
  for (const port of ports.value) {
    errors[port.name] = "";
    if (port.type === "boolean") {
      result[port.name] = flags[port.name];
      continue;
    }
    const raw = (values[port.name] ?? "").trim();
    if (!raw) {
      if (port.required && port.default === undefined) {
        errors[port.name] = t("workflow.run.fieldRequired");
        ok = false;
      }
      continue;
    }
    if (port.type === "number") {
      const number = Number(raw);
      if (Number.isNaN(number)) {
        errors[port.name] = t("workflow.run.invalidNumber");
        ok = false;
      } else result[port.name] = number;
    } else if (port.type === "json" || port.type === "any") {
      try {
        result[port.name] = JSON.parse(raw);
      } catch {
        if (port.type === "any") result[port.name] = raw;
        else {
          errors[port.name] = t("workflow.run.invalidJson");
          ok = false;
        }
      }
    } else if (port.type === "files") {
      result[port.name] = raw.split("\n").map((line) => line.trim()).filter(Boolean);
    } else {
      result[port.name] = port.type === "text" || port.type === "markdown" ? values[port.name] : raw;
    }
  }
  return ok ? result : null;
}

async function submit() {
  error.value = "";
  const inputs = build();
  if (!inputs) return;
  busy.value = true;
  try {
    const run = await backend.startWorkflowRun(props.workflowId, inputs, props.draft ?? false);
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(inputs));
    } catch {
      // 存不下就算了
    }
    emit("started", run);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <UiModal @close="emit('close')">
    <form class="modal run-dialog" @submit.prevent="submit">
      <header class="run-dialog-head">
        <h2>{{ draft ? t("workflow.run.dialogDraftTitle") : t("workflow.run.dialogTitle") }}</h2>
        <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="emit('close')"><X :size="16" /></UiButton>
      </header>
      <p v-if="draft" class="run-dialog-hint">{{ t("workflow.run.dialogDraftHint") }}</p>
      <p v-if="!ports.length" class="run-dialog-hint">{{ t("workflow.run.noInputs") }}</p>
      <div class="run-dialog-body">
        <label v-for="port in ports" :key="port.name" class="run-field">
          <span class="run-field-label">
            {{ label(port) }}<em v-if="port.required"> *</em>
            <small>{{ port.type }}</small>
          </span>
          <small v-if="port.description" class="run-field-desc">{{ port.description }}</small>
          <UiSwitch v-if="port.type === 'boolean'" v-model="flags[port.name]" :label="label(port)" />
          <UiTextarea v-else-if="isMultiline(port)" v-model="values[port.name]" :rows="port.type === 'markdown' ? 5 : 4" :class="{ mono: port.type === 'json' || port.type === 'any' }" :placeholder="port.type === 'files' ? t('workflow.run.filesHint') : ''" />
          <UiInput v-else v-model="values[port.name]" :placeholder="port.type === 'url' ? 'https://' : port.type === 'file' ? t('workflow.run.fileHint') : ''" />
          <small v-if="errors[port.name]" class="run-field-error">{{ errors[port.name] }}</small>
        </label>
      </div>
      <p v-if="error" class="run-field-error">{{ error }}</p>
      <footer class="run-dialog-actions">
        <UiButton @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton type="submit" variant="primary" :disabled="busy"><Play :size="14" /> {{ draft ? t("workflow.run.startDraft") : t("workflow.run.start") }}</UiButton>
      </footer>
    </form>
  </UiModal>
</template>

<style scoped>
.run-dialog {
  display: grid;
  gap: var(--spacing-3);
  width: min(520px, 94vw);
  max-height: 86vh;
}
.run-dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.run-dialog-head h2 {
  margin: 0;
}
.run-dialog-hint {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}
.run-dialog-body {
  display: grid;
  gap: var(--spacing-3);
  overflow: auto;
}
.run-field {
  display: grid;
  gap: var(--spacing-1);
}
.run-field-label {
  display: flex;
  align-items: baseline;
  gap: var(--spacing-2);
  font-size: var(--font-size-sm);
}
.run-field-label em {
  color: var(--color-error);
  font-style: normal;
}
.run-field-label small,
.run-field-desc {
  color: var(--color-text-tertiary);
}
.run-field-error {
  color: var(--color-error);
  font-size: var(--font-size-sm);
}
.mono :deep(textarea),
.mono {
  font-family: var(--font-family-mono);
}
.run-dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-2);
}
</style>
