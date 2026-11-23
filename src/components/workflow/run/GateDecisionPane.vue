<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { GitCompare } from "@lucide/vue";
import { t } from "../../../i18n";
import * as backend from "../../../services/backend";
import type { ChatMessage } from "../../../domain/types";
import { defaultGateDecisions, type GateConfig, type NodeRun, type RunDetail } from "../../../domain/workflow";
import { useAppStore } from "../../../stores/app";
import ChatMessageItem from "../../ChatMessageItem.vue";
import MarkdownPreview from "../../workbench/MarkdownPreview.vue";
import { gateActionsFromText, stripGateActions } from "./gate-actions";
import { nodeStatusLabel } from "./run-format";
import ValueViewer from "./ValueViewer.vue";

const props = defineProps<{ detail: RunDetail; nodeRun: NodeRun }>();
const router = useRouter();
const store = useAppStore();

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

function chose(nextNodeId: string | null | undefined, targetId: string): boolean {
  const text = nextNodeId?.trim() ?? "";
  if (!text) return false;
  if (text.startsWith("[")) {
    try {
      const ids = JSON.parse(text) as unknown;
      return Array.isArray(ids) && ids.includes(targetId);
    } catch {
      return false;
    }
  }
  return text === targetId;
}

const priorSources = computed(() => {
  const incoming = new Set(props.detail.definition.edges.filter((edge) => edge.target === props.nodeRun.nodeId).map((edge) => edge.source));
  const seen = new Set<string>();
  const sources: NodeRun[] = [];
  const ordered = [...props.detail.nodeRuns].sort((a, b) => b.iteration - a.iteration || b.attempt - a.attempt);
  for (const item of ordered) {
    if (!incoming.has(item.nodeId) || seen.has(item.nodeId)) continue;
    if (item.status !== "succeeded" && item.status !== "reused") continue;
    if (!chose(item.nextNodeId, props.nodeRun.nodeId)) continue;
    if (!item.content?.trim()) continue;
    seen.add(item.nodeId);
    sources.push(item);
  }
  return sources;
});

const prior = computed<ChatMessage[]>(() => {
  const messages: ChatMessage[] = [];
  for (const item of priorSources.value) {
    const text = stripGateActions(item.content ?? "");
    if (!text) continue;
    const source = props.detail.definition.nodes.find((node) => node.id === item.nodeId);
    const target = (source?.config as { target?: { type?: string; id?: string } } | undefined)?.target;
    messages.push({
      id: `prior:${item.id}`,
      sessionId: item.sessionId ?? "",
      role: "assistant",
      content: text,
      runId: null,
      agentId: target?.type === "agent" ? target.id : null,
      failureReason: null,
      elapsedMs: null,
      messageKind: "message",
      quickActions: [],
      timeline: [],
      attachments: [],
      createdAt: item.completedAt ?? item.createdAt,
    });
  }
  return messages;
});

const priorSession = computed(() => prior.value.find((message) => message.sessionId)?.sessionId ?? "");
const liveReply = computed(() => {
  const sessionId = priorSession.value;
  if (!sessionId) return "";
  const messages = store.messages[sessionId] ?? [];
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role === "assistant" && message.content.trim()) return message.content;
  }
  return "";
});
const agentButtons = computed(() => {
  const known = new Set(decisions.value.map((item) => item.id));
  const parsed = gateActionsFromText(liveReply.value || priorSources.value.map((item) => item.content ?? "").join("\n"));
  return parsed.filter((action) => known.has(action.id));
});
const buttons = computed(() => {
  if (agentButtons.value.length) {
    return agentButtons.value.map((action) => {
      const decision = decisions.value.find((item) => item.id === action.id);
      return { id: action.id, label: action.label, primary: Boolean(decision?.primary), requireComment: decision?.requireComment };
    });
  }
  return decisions.value.map((item) => ({ id: item.id, label: item.label, primary: Boolean(item.primary), requireComment: item.requireComment }));
});

const followup = ref("");
const followBusy = computed(() => {
  const sessionId = priorSession.value;
  if (!sessionId) return false;
  return Object.values(store.runStates).some((run) => run.sessionId === sessionId && (run.status === "queued" || run.status === "running"));
});

onMounted(() => {
  const sessionId = priorSession.value;
  if (sessionId) void store.ensureSessionLoaded(sessionId);
});
watch(priorSession, (sessionId) => {
  if (sessionId) void store.ensureSessionLoaded(sessionId);
});

function sendFollowup() {
  const sessionId = priorSession.value;
  const text = followup.value.trim();
  if (!sessionId || !text) return;
  const ids = decisions.value.map((item) => item.id).join("、");
  store.send(`${text}\n\n请在回复末尾用 multara-actions 代码块给出这次的按钮。id 只能是 ${ids}。label 写成这次具体该点什么。`, sessionId);
  followup.value = "";
}

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
    <div v-if="prior.length" class="gate-prior">
      <span>{{ t("workflow.gate.prior") }}</span>
      <ChatMessageItem
        v-for="message in prior"
        :key="message.id"
        :message="message"
        view-only
        compact
        :enable-replay="false"
        :allow-continue="false"
        :allow-target-change="false"
        :ui-interactive="false"
      />
    </div>
    <MarkdownPreview v-if="body" class="gate-body" :content="body" path="" />

    <div v-if="past" class="gate-past">
      <div>{{ t("workflow.gate.decided", { decision: decisions.find((item) => item.id === past?.decision)?.label ?? past.decision }) }}</div>
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
        <label v-if="priorSession" class="gate-field">
          <span>{{ t("workflow.gate.followup") }}</span>
          <UiTextarea v-model="followup" :rows="2" :placeholder="t('workflow.gate.followupPlaceholder')" />
          <UiButton size="small" :disabled="!followup.trim() || followBusy" @click="sendFollowup">{{ followBusy ? t("workflow.gate.waiting") : t("workflow.gate.followupSend") }}</UiButton>
        </label>
        <UiButton
          v-for="decision in buttons"
          :key="decision.id"
          :variant="decision.primary ? 'primary' : 'default'"
          :disabled="busy"
          @click="decide(decision.id, decision.requireComment)"
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
.gate-prior {
  display: grid;
  gap: var(--spacing-2);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.gate-body {
  position: static;
  inset: auto;
  max-height: 160px;
  overflow: auto;
  padding: 0;
  background: transparent;
  font-size: var(--font-size-sm);
}
.gate-body :deep(.rich-message) {
  width: 100%;
  margin: 0;
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
