<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import * as backend from "../../services/backend";
import WorkspaceFileTree from "../workbench/WorkspaceFileTree.vue";
import WorkflowFilePreview from "./WorkflowFilePreview.vue";

const props = defineProps<{
  workflowId: string;
  /** 指定这次运行；不传则用最近一次 */
  runId?: string;
  /** 只要关联需求，文件和预览由右侧栏自己打开 */
  tasksOnly?: boolean;
}>();

const preview = ref<{ runId: string; path: string } | null>(null);

const app = useAppStore();
const taskIds = ref<string[]>([]);
const fileRunId = ref("");
const ready = ref(false);

const tasks = computed(() => (app.data?.projects ?? []).filter((project) => !project.archivedAt));
const taskOptions = computed(() => tasks.value.map((project) => ({ value: project.id, label: project.name })));

async function load() {
  ready.value = false;
  try {
    const outcome = await backend.workflowOutcome(props.workflowId, props.runId);
    taskIds.value = outcome.taskIds;
    fileRunId.value = props.runId || "";
  } catch {
    taskIds.value = [];
    fileRunId.value = props.runId || "";
  } finally {
    ready.value = true;
  }
}

watch(() => [props.workflowId, props.runId], load, { immediate: true });

async function setTasks(value: string[]) {
  const next = [...value];
  taskIds.value = next;
  try {
    await backend.setWorkflowTasks(props.workflowId, next);
  } catch {
    await load();
  }
}

function listDirectory(path: string) {
  return backend.listWorkflowDirectory(fileRunId.value, path);
}

const selectedPath = ref("");

function openFile(path: string) {
  selectedPath.value = path;
  if (!fileRunId.value) return;
  preview.value = { runId: fileRunId.value, path };
}
</script>

<template>
  <div class="outcome" :class="{ 'has-preview': preview, 'tasks-only': tasksOnly }">
    <section class="tasks">
      <h3>{{ t("workflow.outcome.tasks") }}</h3>
      <p class="hint">{{ t("workflow.outcome.tasksHint") }}</p>
      <UiSelect
        class="task-select"
        multiple
        clearable
        :model-value="taskIds"
        :options="taskOptions"
        :placeholder="t('workflow.outcome.emptyTasks')"
        :empty-text="t('workflow.outcome.emptyTasks')"
        @update:model-value="setTasks"
      />
    </section>
    <section v-if="!tasksOnly" class="files">
      <h3>{{ t("workflow.outcome.files") }}</h3>
      <div v-if="ready && fileRunId" class="file-tree vs-scroll files-tree">
        <WorkspaceFileTree
          :state-key="`wf:${fileRunId}`"
          embedded
          :selected-path="selectedPath"
          :list-directory="listDirectory"
          @open="openFile"
        />
      </div>
      <p v-else class="hint">{{ t("workflow.outcome.emptyFiles") }}</p>
    </section>
    <WorkflowFilePreview
      v-if="preview && !tasksOnly"
      class="preview"
      embedded
      :run-id="preview.runId"
      :path="preview.path"
      @close="preview = null"
    />
  </div>
</template>

<style scoped>
.outcome {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-3);
  min-width: 0;
  min-height: 0;
  height: 100%;
}
.outcome h3 { margin: 0; font-size: var(--font-size-sm); font-weight: var(--font-weight-medium); }
.hint { margin: 0; color: var(--color-text-tertiary); font-size: var(--font-size-xs); line-height: var(--line-height-normal); }
.task-select { margin-top: var(--spacing-2); }
.files {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--spacing-2);
  min-height: 0;
}
.files-tree {
  flex: 1;
  min-height: 80px;
  overflow: auto;
}
.outcome.tasks-only { height: auto; }
.outcome.has-preview .files {
  flex: none;
  max-height: 38%;
}
.preview {
  flex: 1;
  min-height: 0;
}
</style>
