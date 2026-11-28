<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from "vue";
import { Copy, LoaderCircle, X } from "@lucide/vue";
import {
  getRunRawDetail,
  listSessionRawRuns,
  type RawRunDetail,
  type RawRunEvent,
  type RawSessionData,
} from "../../services/backend";
import { t } from "../../i18n";
import { showToast } from "../ui/toast";
import { formatAbsoluteTime } from "../ui/relative-time";
import {
  appendEvents,
  buildRunJson,
  formatCommandLine,
  previewText,
  searchableEventText,
} from "./session-raw";

const props = defineProps<{
  sessionId: string;
  title?: string;
}>();

const emit = defineEmits<{ close: [] }>();

const PAGE_LIMIT = 200;
const BODY_PREVIEW_CHARS = 20_000;

const loading = ref(true);
const error = ref("");
const data = shallowRef<RawSessionData | null>(null);
const activeRunId = ref<string | null>(null);
const detail = ref<RawRunDetail | null>(null);
const detailLoading = ref(false);
const detailError = ref("");
const events = ref<RawRunEvent[]>([]);
const hasMore = ref(false);
const nextAfterSeq = ref<number | null>(null);
const loadingMore = ref(false);
const query = ref("");
const showRawJson = ref(false);

const runs = computed(() => data.value?.runs ?? []);
const activeRun = computed(
  () => runs.value.find((run) => run.id === activeRunId.value) ?? detail.value?.run ?? null,
);
const filteredEvents = computed(() => {
  const needle = query.value.trim().toLowerCase();
  const matched = needle
    ? events.value.filter((event) => searchableEventText(event).includes(needle))
    : events.value;
  return matched.map((event) => ({ event, body: eventBody(event) }));
});
const commandLine = computed(() => {
  const invocation = detail.value?.invocation ?? activeRun.value?.invocation;
  if (!invocation) return "";
  return formatCommandLine(invocation.program, invocation.args);
});
const rawJson = computed(() => {
  if (!detail.value) return "";
  return JSON.stringify(
    buildRunJson({ ...detail.value, events: appendEvents([], events.value) }),
    null,
    2,
  );
});
const hasLegacyRun = computed(() => Boolean(detail.value && !detail.value.invocation));

async function loadSession() {
  loading.value = true;
  error.value = "";
  try {
    const payload = await listSessionRawRuns(props.sessionId);
    data.value = payload;
    const latest = payload.runs[payload.runs.length - 1];
    if (latest) await selectRun(latest.id);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading.value = false;
  }
}

async function selectRun(runId: string) {
  if (activeRunId.value === runId && detail.value) return;
  activeRunId.value = runId;
  detailLoading.value = true;
  detailError.value = "";
  showRawJson.value = false;
  try {
    const payload = await getRunRawDetail(runId, null, PAGE_LIMIT);
    detail.value = payload;
    events.value = payload.events;
    hasMore.value = payload.hasMore;
    nextAfterSeq.value = payload.nextAfterSeq;
  } catch (cause) {
    detailError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    detailLoading.value = false;
  }
}

async function loadMore() {
  const runId = activeRunId.value;
  if (!runId || loadingMore.value) return;
  loadingMore.value = true;
  try {
    const payload = await getRunRawDetail(runId, nextAfterSeq.value, PAGE_LIMIT);
    events.value = appendEvents(events.value, payload.events);
    hasMore.value = payload.hasMore;
    nextAfterSeq.value = payload.nextAfterSeq;
  } catch (cause) {
    showToast(cause instanceof Error ? cause.message : String(cause), "error");
  } finally {
    loadingMore.value = false;
  }
}

async function copy(text: string) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    showToast(t("raw.copied"));
  } catch {
    showToast(t("raw.copyFailed"), "error");
  }
}

function eventBody(event: RawRunEvent) {
  const text = [event.content, event.inputJson, event.output, event.outputJson]
    .filter((value): value is string => Boolean(value))
    .join("\n");
  return previewText(text, BODY_PREVIEW_CHARS);
}

function runStatusTone(status: string) {
  if (status === "running" || status === "queued") return "is-live";
  if (status === "failed" || status === "cancelled" || status === "stopped") return "is-bad";
  return "is-done";
}

onMounted(loadSession);
</script>

<template>
  <UiModal @click.self="emit('close')">
    <div class="modal session-raw-dialog">
      <header class="modal-heading">
        <div class="session-raw-heading">
          <h2>{{ title?.trim() || data?.session?.title || t("raw.title") }}</h2>
          <span v-if="data?.session" class="session-raw-subtitle">
            {{ t("raw.runCount", { n: runs.length }) }} · {{ data.session.targetType }}/{{
              data.session.targetId
            }}
          </span>
        </div>
        <UiButton size="icon" class="icon-button" :tip="t('common.close')" @click="emit('close')">
          <X :size="15" />
        </UiButton>
      </header>

      <div v-if="loading" class="session-raw-state">
        <LoaderCircle class="session-raw-spin" :size="18" />
        <span>{{ t("raw.loading") }}</span>
      </div>

      <div v-else-if="error" class="session-raw-state is-error">
        <span>{{ t("raw.loadFailed", { error }) }}</span>
        <UiButton size="small" @click="loadSession">{{ t("raw.retry") }}</UiButton>
      </div>

      <div v-else-if="!runs.length" class="session-raw-state">
        <span>{{ t("raw.empty") }}</span>
      </div>

      <div v-else class="session-raw-body">
        <aside class="session-raw-runs">
          <button
            v-for="(run, index) in runs"
            :key="run.id"
            type="button"
            class="session-raw-run"
            :class="{ 'is-active': run.id === activeRunId }"
            @click="selectRun(run.id)"
          >
            <span class="session-raw-run-head">
              <i class="session-raw-dot" :class="runStatusTone(run.status)" />
              <span class="session-raw-run-index">#{{ index + 1 }}</span>
              <span class="session-raw-run-agent">{{ run.agentId }}</span>
              <span class="session-raw-run-time">{{ formatAbsoluteTime(run.createdAt) }}</span>
            </span>
            <span class="session-raw-run-cmd">
              {{ run.invocation ? run.invocation.program : t("raw.noInvocationShort") }}
            </span>
            <span class="session-raw-run-meta">
              {{ run.status }} · {{ t("raw.eventCount", { n: run.eventCount }) }}
            </span>
          </button>
        </aside>

        <section class="session-raw-detail vs-scroll">
          <div v-if="detailLoading" class="session-raw-state">
            <LoaderCircle class="session-raw-spin" :size="18" />
            <span>{{ t("raw.loading") }}</span>
          </div>

          <div v-else-if="detailError" class="session-raw-state is-error">
            <span>{{ t("raw.loadFailed", { error: detailError }) }}</span>
            <UiButton size="small" @click="activeRunId && selectRun(activeRunId)">
              {{ t("raw.retry") }}
            </UiButton>
          </div>

          <template v-else-if="detail">
            <div class="session-raw-meta">
              <span>{{ detail.run.id }}</span>
              <span>{{ detail.run.runtimeId }}</span>
              <span>{{ detail.run.status }}</span>
              <span v-if="detail.run.mode">{{ t("raw.mode") }}: {{ detail.run.mode }}</span>
              <span v-if="detail.run.effort">{{ t("raw.effort") }}: {{ detail.run.effort }}</span>
              <span v-if="detail.run.usageModel || detail.run.modelOverride">
                {{ t("raw.model") }}: {{ detail.run.usageModel || detail.run.modelOverride }}
              </span>
              <span v-if="detail.run.inputTokens != null">
                {{ t("raw.tokens", { input: detail.run.inputTokens, output: detail.run.outputTokens ?? 0 }) }}
              </span>
              <span v-if="detail.run.costUsd">{{ t("raw.cost") }}: ${{ detail.run.costUsd.toFixed(4) }}</span>
            </div>

            <div v-if="hasLegacyRun" class="session-raw-notice">{{ t("raw.noInvocation") }}</div>

            <section v-if="commandLine" class="session-raw-block">
              <header>
                <h3>{{ t("raw.command") }}</h3>
                <UiButton size="small" variant="ghost" @click="copy(commandLine)">
                  <Copy :size="13" />{{ t("raw.copy") }}
                </UiButton>
              </header>
              <pre class="session-raw-code">{{ commandLine }}</pre>
              <div class="session-raw-tags">
                <span v-if="detail.invocation">
                  {{ t("raw.delivery") }}: {{ detail.invocation.delivery }}
                </span>
                <span v-if="detail.invocation?.cwd">{{ t("raw.cwd") }}: {{ detail.invocation.cwd }}</span>
                <span v-if="detail.invocation?.resumeId">
                  {{ t("raw.resume") }}: {{ detail.invocation.resumeId }}
                </span>
              </div>
            </section>

            <section v-if="detail.userMessage" class="session-raw-block">
              <header>
                <h3>{{ t("raw.userMessage") }}</h3>
                <UiButton
                  size="small"
                  variant="ghost"
                  @click="copy(detail.userMessage?.content ?? '')"
                >
                  <Copy :size="13" />{{ t("raw.copy") }}
                </UiButton>
              </header>
              <pre class="session-raw-code">{{ detail.userMessage.content }}</pre>
            </section>

            <section v-if="detail.invocation?.prompt" class="session-raw-block">
              <header>
                <h3>
                  {{ t("raw.prompt") }}
                  <small>{{ t("raw.charCount", { n: detail.invocation.promptChars }) }}</small>
                </h3>
                <UiButton size="small" variant="ghost" @click="copy(detail.invocation?.prompt ?? '')">
                  <Copy :size="13" />{{ t("raw.copy") }}
                </UiButton>
              </header>
              <div v-if="detail.invocation.promptTruncated" class="session-raw-notice">
                {{ t("raw.promptTruncated", { n: detail.invocation.promptChars }) }}
              </div>
              <pre class="session-raw-code is-prompt">{{ detail.invocation.prompt }}</pre>
            </section>

            <section class="session-raw-block">
              <header>
                <h3>{{ t("raw.assistant") }}</h3>
                <span class="session-raw-count">{{ detail.assistantMessages.length }}</span>
              </header>
              <p v-if="!detail.assistantMessages.length" class="session-raw-muted">
                {{ t("raw.noAssistant") }}
              </p>
              <article
                v-for="message in detail.assistantMessages"
                :key="message.id"
                class="session-raw-message"
              >
                <div class="session-raw-message-meta">
                  <span>{{ message.messageKind }}</span>
                  <span>{{ formatAbsoluteTime(message.createdAt) }}</span>
                  <span v-if="message.elapsedMs">{{ t("raw.elapsed", { n: message.elapsedMs }) }}</span>
                  <strong v-if="message.failureReason">{{ message.failureReason }}</strong>
                </div>
                <pre class="session-raw-code">{{ message.content }}</pre>
              </article>
            </section>

            <section class="session-raw-block">
              <header>
                <h3>{{ t("raw.events") }}</h3>
                <span class="session-raw-count">{{ detail.eventCount }}</span>
              </header>
              <div class="session-raw-tools">
                <UiInput
                  v-model="query"
                  :placeholder="t('raw.searchEvents')"
                  class="session-raw-search"
                />
                <UiButton size="small" variant="ghost" @click="showRawJson = !showRawJson">
                  {{ showRawJson ? t("raw.showTimeline") : t("raw.showJson") }}
                </UiButton>
              </div>
              <pre v-if="showRawJson" class="session-raw-code is-json">{{ rawJson }}</pre>
              <template v-else>
                <p v-if="!events.length" class="session-raw-muted">{{ t("raw.noEvents") }}</p>
                <p v-else-if="!filteredEvents.length" class="session-raw-muted">
                  {{ t("raw.noMatchedEvents") }}
                </p>
                <ol v-else class="session-raw-event-list">
                  <li
                    v-for="row in filteredEvents"
                    :key="row.event.seq"
                    class="session-raw-event"
                    :class="`is-${row.event.eventType}`"
                  >
                    <div class="session-raw-event-head">
                      <span class="session-raw-seq">{{ row.event.seq }}</span>
                      <span class="session-raw-event-type">{{ row.event.eventType }}</span>
                      <span v-if="row.event.tool" class="session-raw-event-tool">{{ row.event.tool }}</span>
                      <span v-if="row.event.status" class="session-raw-event-status">{{ row.event.status }}</span>
                      <span class="session-raw-event-time">{{ formatAbsoluteTime(row.event.createdAt) }}</span>
                    </div>
                    <div v-if="row.event.callId || row.event.parentCallId" class="session-raw-event-ids">
                      <span v-if="row.event.callId">{{ t("raw.callId") }}: {{ row.event.callId }}</span>
                      <span v-if="row.event.parentCallId">
                        {{ t("raw.parentCallId") }}: {{ row.event.parentCallId }}
                      </span>
                    </div>
                    <pre v-if="row.body.text" class="session-raw-code">{{ row.body.text }}</pre>
                    <span v-if="row.body.clipped" class="session-raw-muted">
                      {{ t("raw.bodyClipped") }}
                    </span>
                  </li>
                </ol>
                <UiButton v-if="hasMore" size="small" :disabled="loadingMore" @click="loadMore">
                  <LoaderCircle v-if="loadingMore" class="session-raw-spin" :size="13" />
                  {{ t("raw.loadMore") }}
                </UiButton>
              </template>
            </section>
          </template>
        </section>
      </div>
    </div>
  </UiModal>
</template>

<style scoped>
.session-raw-dialog {
  display: flex;
  flex-direction: column;
  width: min(1120px, 100%);
  height: min(760px, calc(100vh - 2 * var(--spacing-5)));
  overflow: hidden;
  padding-bottom: 0;
}

.session-raw-heading {
  display: grid;
  gap: 2px;
  min-width: 0;
}

.session-raw-subtitle {
  color: var(--color-text-secondary);
  font-size: var(--font-size-xs);
}

.session-raw-state {
  display: flex;
  flex: 1 1 auto;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-2);
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}

.session-raw-state.is-error {
  color: var(--color-error);
}

.session-raw-spin {
  animation: spin 1s linear infinite;
}

.session-raw-body {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  gap: var(--spacing-4);
  flex: 1 1 auto;
  min-height: 0;
  padding-bottom: var(--spacing-5);
}

.session-raw-runs {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-height: 0;
  overflow: auto;
  padding-right: var(--spacing-1);
}

.session-raw-run {
  display: grid;
  gap: 2px;
  padding: var(--spacing-2);
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  background: none;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.session-raw-run:hover {
  background: var(--color-surface-hover);
}

.session-raw-run.is-active {
  border-color: var(--color-border);
  background: var(--color-surface-hover);
}

.session-raw-run-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-run-agent {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--color-text);
}

.session-raw-run-time {
  margin-left: auto;
  flex-shrink: 0;
}

.session-raw-run-cmd {
  overflow: hidden;
  font-size: var(--font-size-sm);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-raw-run-meta {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--color-text-secondary);
}

.session-raw-dot.is-live {
  background: var(--color-warning, #e0a458);
}

.session-raw-dot.is-bad {
  background: var(--color-error);
}

.session-raw-dot.is-done {
  background: var(--color-success, #4caf50);
}

.session-raw-detail {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-4);
  min-height: 0;
  overflow: auto;
  padding-right: var(--spacing-2);
}

.session-raw-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 12px;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-notice {
  padding: var(--spacing-2);
  border-radius: var(--radius-md);
  background: var(--color-surface-hover);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-block {
  display: grid;
  gap: var(--spacing-2);
}

.session-raw-block > header {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
}

.session-raw-block h3 {
  margin: 0;
  font-size: var(--font-size-sm);
  font-weight: var(--font-weight-semibold);
}

.session-raw-block h3 small {
  margin-left: 6px;
  font-weight: var(--font-weight-regular);
  color: var(--color-text-secondary);
}

.session-raw-block > header .ui-button {
  margin-left: auto;
}

.session-raw-count {
  margin-left: auto;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-code {
  margin: 0;
  padding: var(--spacing-2) var(--spacing-3);
  overflow: auto;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  font-family: var(--font-family-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
  font-size: var(--font-size-xs);
  line-height: 1.55;
  white-space: pre-wrap;
  word-break: break-word;
}

.session-raw-code.is-prompt,
.session-raw-code.is-json {
  max-height: 320px;
}

.session-raw-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-message {
  display: grid;
  gap: var(--spacing-1);
}

.session-raw-message-meta {
  display: flex;
  gap: var(--spacing-2);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-message-meta strong {
  color: var(--color-error);
}

.session-raw-tools {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
}

.session-raw-search {
  flex: 1 1 auto;
}

.session-raw-event-list {
  display: grid;
  gap: var(--spacing-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.session-raw-event {
  display: grid;
  gap: 4px;
  padding: var(--spacing-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface);
}

.session-raw-event-head {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  font-size: var(--font-size-xs);
}

.session-raw-seq {
  color: var(--color-text-secondary);
  font-variant-numeric: tabular-nums;
}

.session-raw-event-type {
  font-weight: var(--font-weight-medium);
}

.session-raw-event-tool {
  padding: 0 6px;
  border-radius: var(--radius-sm);
  background: var(--color-surface-hover);
}

.session-raw-event-status {
  color: var(--color-text-secondary);
}

.session-raw-event-time {
  margin-left: auto;
  color: var(--color-text-secondary);
}

.session-raw-event-ids {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.session-raw-muted {
  margin: 0;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
