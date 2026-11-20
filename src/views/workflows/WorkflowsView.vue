<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { Copy, GitBranch, History, Plus, Trash2, Upload } from "@lucide/vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { confirmDialog } from "../../components/ui/confirm-dialog";
import { runStatusLabel, runStatusTone } from "../../components/workflow/run/run-format";
import { useWorkflowsStore } from "../../stores/workflows";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import type { WorkflowSummary } from "../../domain/workflow";

const router = useRouter();
const store = useWorkflowsStore();
const error = ref("");
const loading = ref(true);
const togglingIds = ref(new Set<string>());

const createOpen = ref(false);
const createName = ref("");
const createDesc = ref("");
const createTemplate = ref("");
const creating = ref(false);
const createError = ref("");

const templateOptions = computed(() => [
  { value: "", label: t("workflow.list.blank") },
  ...store.flowTemplates.map((item) => ({ value: item.id, label: item.name, description: item.description })),
]);

async function refresh() {
  try {
    await Promise.all([store.loadList(), store.loadTemplates()]);
    error.value = "";
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading.value = false;
  }
}

let unlisten: UnlistenFn | null = null;
onMounted(async () => {
  await refresh();
  try {
    unlisten = await listen("workflow-run-changed", () => void store.loadList().catch(() => undefined));
  } catch {
    // 浏览器预览没有事件总线
  }
});
onActivated(() => void refresh());
onBeforeUnmount(() => unlisten?.());

function openCreate() {
  createName.value = "";
  createDesc.value = "";
  createTemplate.value = "";
  createError.value = "";
  createOpen.value = true;
}

async function create() {
  if (!createName.value.trim()) return;
  creating.value = true;
  createError.value = "";
  try {
    const created = await backend.createWorkflow({
      name: createName.value.trim(),
      description: createDesc.value.trim(),
      templateId: createTemplate.value || null,
    });
    createOpen.value = false;
    await router.push(`/workflows/${created.id}`);
  } catch (cause) {
    createError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    creating.value = false;
  }
}

async function importFlow() {
  error.value = "";
  try {
    const path = await backend.pickWorkflowImportFile();
    if (!path) {
      if (!backend.hasBackend()) error.value = t("workflow.desktopOnly");
      return;
    }
    const created = await backend.importWorkflow(path);
    await router.push(`/workflows/${created.id}`);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function setEnabled(item: WorkflowSummary, enabled: boolean) {
  if (togglingIds.value.has(item.id)) return;
  togglingIds.value = new Set(togglingIds.value).add(item.id);
  try {
    await backend.setWorkflowEnabled(item.id, enabled);
    await store.loadList();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    const next = new Set(togglingIds.value);
    next.delete(item.id);
    togglingIds.value = next;
  }
}

async function duplicate(item: WorkflowSummary) {
  try {
    await backend.duplicateWorkflow(item.id);
    await store.loadList();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function remove(item: WorkflowSummary) {
  if (!(await confirmDialog(t("workflow.list.deleteConfirm", { name: item.name }), { danger: true }))) return;
  try {
    await backend.deleteWorkflow(item.id);
    await store.loadList();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function formatTime(value: string | null) {
  return value ? new Date(value).toLocaleString() : "—";
}
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>{{ t("workflow.list.title") }}</h1>
        <p>{{ t("workflow.list.subtitle") }}</p>
      </div>
      <div class="header-actions">
        <UiButton @click="importFlow"><Upload :size="15" /> {{ t("workflow.list.import") }}</UiButton>
        <UiButton variant="primary" @click="openCreate"><Plus :size="16" /> {{ t("workflow.list.create") }}</UiButton>
      </div>
    </div>

    <div v-if="error" class="error-card">{{ error }}</div>
    <div v-if="loading" class="empty">{{ t("app.loading") }}</div>
    <section v-else-if="!store.list.length" class="panel placeholder">
      <span class="ds-empty-icon"><GitBranch :size="18" /></span>
      <h2>{{ t("workflow.list.empty") }}</h2>
      <p>{{ t("workflow.list.emptyHint") }}</p>
    </section>
    <div v-else class="card-list">
      <article v-for="item in store.list" :key="item.id" class="list-card wf-card" @click="router.push(`/workflows/${item.id}`)">
        <span class="wf-card-icon"><GitBranch :size="16" /></span>
        <div class="grow">
          <strong>{{ item.name }}</strong>
          <small>
            <template v-if="item.publishedVersion">v{{ item.publishedVersion }}</template>
            <template v-else>{{ t("workflow.list.unpublished") }}</template>
            <template v-if="item.hasUnpublishedChanges && item.publishedVersion"> · {{ t("workflow.list.hasChanges") }}</template>
            · {{ t("workflow.list.nodes", { count: item.nodeCount }) }}
          </small>
          <p v-if="item.description">{{ item.description }}</p>
          <div class="wf-card-times">
            <span>{{ t("workflow.list.lastRun", { time: formatTime(item.lastRunAt) }) }}</span>
            <span v-if="item.lastRunStatus" class="ds-badge" :data-tone="runStatusTone(item.lastRunStatus)">{{ runStatusLabel(item.lastRunStatus) }}</span>
          </div>
        </div>
        <div class="wf-card-actions" @click.stop>
          <UiTooltip :content="item.publishedVersion ? (item.enabled ? t('common.enabledOn') : t('common.enabledOff')) : t('workflow.list.enableNeedsPublish')">
            <UiSwitch
              :model-value="item.enabled"
              :busy="togglingIds.has(item.id)"
              :disabled="!item.publishedVersion"
              :label="t('workflow.list.enable')"
              @update:model-value="setEnabled(item, $event)"
            />
          </UiTooltip>
          <UiButton variant="ghost" size="icon" :tip="t('workflow.list.runs')" @click="router.push(`/workflows/${item.id}/runs`)"><History :size="14" /></UiButton>
          <UiButton variant="ghost" size="icon" :tip="t('workflow.list.duplicate')" @click="duplicate(item)"><Copy :size="14" /></UiButton>
          <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(item)"><Trash2 :size="14" /></UiButton>
        </div>
      </article>
    </div>

    <UiModal v-if="createOpen" @close="createOpen = false">
      <form class="modal wf-create" @submit.prevent="create">
        <h2>{{ t("workflow.list.create") }}</h2>
        <label>
          {{ t("workflow.list.name") }}
          <UiInput v-model="createName" required autofocus :placeholder="t('workflow.list.namePlaceholder')" />
        </label>
        <label>
          {{ t("workflow.list.description") }}
          <UiTextarea v-model="createDesc" rows="2" />
        </label>
        <label>
          {{ t("workflow.list.template") }}
          <UiSelect v-model="createTemplate" :options="templateOptions" />
        </label>
        <p v-if="createError" class="wf-create-error">{{ createError }}</p>
        <div class="wf-create-actions">
          <UiButton @click="createOpen = false">{{ t("common.cancel") }}</UiButton>
          <UiButton type="submit" variant="primary" :disabled="creating">{{ t("common.create") }}</UiButton>
        </div>
      </form>
    </UiModal>
  </div>
</template>

<style scoped>
.wf-card { cursor: pointer; align-items: flex-start; }
.wf-card:hover { background: var(--color-surface-hover); }
.wf-card-icon { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; flex: none; border-radius: var(--radius-md); background: var(--color-purple-subtle); color: var(--color-purple); }
.wf-card p { margin: 0; color: var(--color-text-secondary); font-size: var(--font-size-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.wf-card small { color: var(--color-text-tertiary); font-size: var(--font-size-xs); }
.wf-card-times { display: flex; gap: var(--spacing-2); align-items: center; font-size: var(--font-size-xs); color: var(--color-text-tertiary); }
.wf-card-actions { display: flex; align-items: center; gap: var(--spacing-1); flex: none; }
.wf-create { display: grid; gap: var(--spacing-3); width: min(440px, 90vw); }
.wf-create h2 { margin: 0; font-size: var(--font-size-xl); }
.wf-create label { display: grid; gap: var(--spacing-1); font-size: var(--font-size-sm); color: var(--color-text-secondary); }
.wf-create-actions { display: flex; justify-content: flex-end; gap: var(--spacing-2); }
.wf-create-error { color: var(--color-error); font-size: var(--font-size-sm); margin: 0; }
</style>
