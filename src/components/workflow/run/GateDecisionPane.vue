<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { GitCompare } from "@lucide/vue";
import { t } from "../../../i18n";
import * as backend from "../../../services/backend";
import { defaultGateDecisions, type GateConfig, type NodeRun, type RunDetail } from "../../../domain/workflow";
import MarkdownPreview from "../../workbench/MarkdownPreview.vue";
import { nodeStatusLabel } from "./run-format";
import ValueViewer from "./ValueViewer.vue";

const props = defineProps<{ detail: RunDetail; nodeRun: NodeRun }>();
const router = useRouter();

interface FormField { key: string; type: string; title: string; options: string[]; required: boolean }

const node = computed(() => props.detail.definition.nodes.find((item) => item.id === props.nodeRun.nodeId));
const config = computed(() => (node.value?.config ?? {}) as Partial<GateConfig>);
const decisions = computed(() => (config.value.decisions?.length ? config.value.decisions : defaultGateDecisions()));
const past = computed(() => props.detail.gates.find((item) => item.nodeRunId === props.nodeRun.id));
const waiting = computed(() => props.nodeRun.status === "waiting_human" && !past.value);
const state = computed(() => (props.nodeRun.state ?? {}) as Record<string, unknown>);
// 引擎若把渲染后的标题 / 正文放进节点运行 state 就优先用，否则显示配置里的模板原文
const title = computed(() => String(state.value.title ?? config.value.title ?? node.value?.title ?? ""));
const body = computed(() => String(state.value.body ?? config.value.body ?? ""));

const fields = computed<FormField[]>(() => {
  const schema = config.value.form as { properties?: Record<string, Record<string, unknown>>; required?: string[] } | undefined;
  if (!schema?.properties) return [];
  return Object.entries(schema.properties).map(([key, prop]) => ({
    key,
    type: Array.isArray(prop.enum) ? "enum" : String(prop.type ?? "string"),
    title: String(prop.title ?? key),
    options: Array.isArray(prop.enum) ? prop.enum.map(String) : [],
    required: Boolean(schema.required?.includes(key)),
  }));
});

const form = reactive<Record<string, unknown>>({});
const jsonDrafts = reactive<Record<string, string>>({});
const comment = ref("");
const busy = ref(false);
const error = ref("");

watch(fields, (list) => {
  for (const field of list) {
    if (field.key in form) continue;
    if (field.type === "boolean") form[field.key] = false;
    else if (field.type === "enum") form[field.key] = field.options[0] ?? "";
    else form[field.key] = "";
  }
}, { immediate: true });

const slotProjects = computed(() => {
  const slots = config.value.showDiffOf ?? [];
  return slots
    .map((key) => props.detail.workspaces.find((item) => item.slotKey === key))
    .filter((item): item is NonNullable<typeof item> => Boolean(item));
});

function collectForm(): { ok: boolean; value: Record<string, unknown> } {
  const value: Record<string, unknown> = {};
  for (const field of fields.value) {
    let item = form[field.key];
    if (field.type === "number" || field.type === "integer") {
      item = item === "" || item === null ? undefined : Number(item);
    } else if (field.type === "object" || field.type === "array") {
      const raw = jsonDrafts[field.key] ?? "";
      if (raw.trim()) {
        try {
          item = JSON.parse(raw);
        } catch {
          error.value = t("workflow.gate.invalidJson", { field: field.title });
          return { ok: false, value };
        }
      } else item = undefined;
    }
    if (field.required && (item === undefined || item === "")) {
      error.value = t("workflow.gate.fieldRequired", { field: field.title });
      return { ok: false, value };
    }
    if (item !== undefined) value[field.key] = item;
  }
  return { ok: true, value };
}

async function decide(port: string, requireComment: boolean | undefined) {
  error.value = "";
  if (requireComment && !comment.value.trim()) {
    error.value = t("workflow.gate.commentRequired");
    return;
  }
  const collected = collectForm();
  if (!collected.ok) return;
  busy.value = true;
  try {
    await backend.decideWorkflowGate(props.nodeRun.id, port, comment.value.trim(), collected.value);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

function openDiff(projectId: string) {
  void router.push(`/projects/${projectId}`);
}
</script>

<template>
  <section class="gate-pane">
    <header class="gate-head">
      <strong>{{ title }}</strong>
      <span class="ds-badge" :data-tone="waiting ? 'warning' : undefined">{{ waiting ? t("workflow.gate.waiting") : nodeStatusLabel(nodeRun.status) }}</span>
    </header>
    <MarkdownPreview v-if="body" class="gate-body" :content="body" path="" />

    <div v-if="past" class="gate-past">
      <div>{{ t("workflow.gate.decided", { decision: decisions.find((item) => item.port === past?.decision)?.label ?? past.decision }) }}</div>
      <p v-if="past.comment" class="gate-comment">{{ past.comment }}</p>
      <ValueViewer v-if="past.form && Object.keys(past.form as object).length" :value="past.form" type="json" />
      <small>{{ past.source }} · {{ new Date(past.decidedAt).toLocaleString() }}</small>
    </div>

    <template v-if="waiting">
      <div v-if="fields.length" class="gate-form">
        <label v-for="field in fields" :key="field.key" class="gate-field">
          <span>{{ field.title }}<em v-if="field.required"> *</em></span>
          <UiSelect v-if="field.type === 'enum'" v-model="form[field.key]" :options="field.options.map((value) => ({ value, label: value }))" />
          <UiSwitch v-else-if="field.type === 'boolean'" v-model="form[field.key] as boolean" :label="field.title" />
          <UiInput v-else-if="field.type === 'number' || field.type === 'integer'" v-model="form[field.key]" type="number" />
          <UiTextarea v-else-if="field.type === 'object' || field.type === 'array'" v-model="jsonDrafts[field.key]" :rows="3" />
          <UiInput v-else v-model="form[field.key]" />
        </label>
      </div>
      <label class="gate-field">
        <span>{{ t("workflow.gate.comment") }}</span>
        <UiTextarea v-model="comment" :rows="2" :placeholder="t('workflow.gate.commentPlaceholder')" />
      </label>
      <p v-if="error" class="gate-error">{{ error }}</p>
      <div class="gate-actions">
        <UiButton
          v-for="decision in decisions"
          :key="decision.port"
          :variant="decision.primary ? 'primary' : 'default'"
          :disabled="busy"
          @click="decide(decision.port, decision.requireComment)"
        >{{ decision.label }}</UiButton>
        <UiButton v-for="slot in slotProjects" :key="slot.slotKey" variant="ghost" @click="openDiff(slot.projectId)">
          <GitCompare :size="14" /> {{ t("workflow.gate.viewDiff", { slot: slot.slotKey }) }}
        </UiButton>
      </div>
    </template>
  </section>
</template>

<style scoped>
.gate-pane {
  display: grid;
  gap: var(--spacing-3);
  padding: var(--spacing-3);
  border: 1px solid var(--color-warning-border);
  border-radius: var(--radius-lg);
  background: var(--color-warning-subtle);
}
.gate-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-2);
}
.gate-body {
  max-height: 240px;
  overflow: auto;
  font-size: var(--font-size-md);
}
.gate-past,
.gate-form {
  display: grid;
  gap: var(--spacing-2);
  font-size: var(--font-size-sm);
}
.gate-past small {
  color: var(--color-text-tertiary);
}
.gate-comment {
  margin: 0;
  white-space: pre-wrap;
  color: var(--color-text-secondary);
}
.gate-field {
  display: grid;
  gap: var(--spacing-1);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.gate-field em {
  color: var(--color-error);
  font-style: normal;
}
.gate-error {
  margin: 0;
  color: var(--color-error);
  font-size: var(--font-size-sm);
}
.gate-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--spacing-2);
}
</style>
