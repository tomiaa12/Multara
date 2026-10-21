<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { Maximize2, Minimize2, Paperclip, X } from "@lucide/vue";
import type { EditorSelectionContext, TaskStatus } from "../domain/types";
import { isMiku } from "../domain/types";
import { useAppStore } from "../stores/app";
import * as backend from "../services/backend";
import ChatComposer from "../components/chat/ChatComposer.vue";
import { formatAgentPrompt, type ComposerNode } from "../components/chat/composer-content";
import {
  resolvePathInWorkspaces,
  type OsDroppedEntry,
  type WorkspaceFileDragPayload,
} from "../components/chat/workspace-file-drag";
import { decodeTextBytes } from "../components/workbench/text-encoding";
import TagManageDialog from "../components/ui/TagManageDialog.vue";
import TagPicker from "../components/ui/TagPicker.vue";
import {
  addRequirementCategory,
  loadRequirementCategories,
  recolorRequirementCategory,
  removeRequirementCategory,
  renameRequirementCategory,
  saveRequirementCategories,
  type RequirementCategoryState,
} from "./dashboard/requirement-categories";
import { TASK_COLUMNS } from "./task-board";
import { writeTaskMeta } from "./task-meta";

const props = defineProps<{
  initialStatus: TaskStatus;
}>();

const emit = defineEmits<{
  close: [];
}>();

const store = useAppStore();
const busy = ref(false);
const error = ref("");
const expanded = ref(false);
const keepOpen = ref(false);
const showTagManage = ref(false);
const categories = ref<RequirementCategoryState>(loadRequirementCategories(localStorage));
const contexts = ref<EditorSelectionContext[]>([]);
const composer = ref<{
  clear: () => void;
  insertChip: (item: EditorSelectionContext) => void;
  focus: () => void;
  serialize: () => { text: string; nodes: ComposerNode[]; empty: boolean };
} | null>(null);

const form = reactive({
  title: "",
  status: props.initialStatus,
  agentId: "",
  projectId: "",
  labels: [] as string[],
});

const agents = computed(() => store.data?.agents ?? []);
const squads = computed(() => store.data?.squads ?? []);
const projects = computed(() => store.data?.projects ?? []);
const project = computed(() => projects.value.find((item) => item.id === form.projectId));
const workspaces = computed(() => {
  const all = store.data?.workspaces ?? [];
  return form.projectId ? all.filter((item) => item.projectId === form.projectId) : all;
});
const assigneeId = computed(() => {
  if (form.agentId.startsWith("squad:")) {
    const squadId = form.agentId.slice("squad:".length);
    return squads.value.find((item) => item.id === squadId)?.leaderAgentId ?? "";
  }
  return form.agentId.replace(/^agent:/, "");
});
const targetOptions = computed(() => [
  ...agents.value.map((agent) => ({ value: `agent:${agent.id}`, label: agent.name, avatar: agent.avatar })),
  ...squads.value.map((squad) => ({
    value: `squad:${squad.id}`,
    label: `小队 · ${squad.name}`,
    avatar: agents.value.find((agent) => agent.id === squad.leaderAgentId)?.avatar ?? "◉",
  })),
]);
const statusOptions = computed(() => TASK_COLUMNS.map((column) => ({ value: column.id, label: column.label })));
const projectOptions = computed(() => [
  {
    value: "__associate_hint",
    label: "关联后，这个任务在该需求的工作区里执行，智能体可以直接改这些目录里的代码。",
    disabled: true,
  },
  ...projects.value.map((item) => ({ value: item.id, label: item.name })),
]);
const canCreate = computed(() => Boolean(form.title.trim() && assigneeId.value && !busy.value));

watch(agents, (list) => {
  if (form.agentId && targetOptions.value.some((item) => item.value === form.agentId)) return;
  const miku = list.find((item) => isMiku(item)) ?? list[0];
  form.agentId = miku ? `agent:${miku.id}` : "";
}, { immediate: true });

function persistCategories(next: RequirementCategoryState) {
  categories.value = next;
  saveRequirementCategories(localStorage, next);
}

function createCategory(name: string) {
  const id = crypto.randomUUID();
  const next = addRequirementCategory(categories.value, name, id);
  if (next === categories.value) return "";
  persistCategories(next);
  return id;
}

function onTagCreate(name: string) {
  const id = createCategory(name);
  if (id) form.labels = [...form.labels, id];
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));

function rememberChip(context: EditorSelectionContext) {
  const existing = contexts.value.find((item) => item.path === context.path && item.startLine === context.startLine);
  contexts.value = existing
    ? contexts.value.map((item) => (item.id === existing.id ? { ...context, id: existing.id } : item))
    : [...contexts.value, context].slice(-8);
  composer.value?.insertChip(existing ? { ...context, id: existing.id } : context);
  composer.value?.focus();
}

async function addWorkspaceChip(file: { workspaceId: string; path: string; isDirectory?: boolean }) {
  let text = "";
  let isDirectory = Boolean(file.isDirectory);
  try {
    if (isDirectory) {
      const entries = await backend.listWorkspaceDirectory(file.workspaceId, file.path);
      text = entries.map((entry) => (entry.isDirectory ? `${entry.name}/` : entry.name)).join("\n");
    } else {
      const content = await backend.readWorkspaceFile(file.workspaceId, file.path);
      text = content.content;
    }
  } catch {
    const name = file.path.split(/[\\/]/).pop() || file.path;
    error.value = `未能读取 ${name}，已添加空引用`;
  }
  rememberChip({
    id: crypto.randomUUID(),
    workspaceId: file.workspaceId,
    path: file.path,
    startLine: 0,
    endLine: isDirectory ? 0 : Math.max(1, text.split(/\r\n|\r|\n/).length),
    text,
    isDirectory,
  });
}

async function addExternalChip(entry: OsDroppedEntry) {
  const path = entry.path || entry.name || "外部文件";
  const resolved = entry.path ? resolvePathInWorkspaces(entry.path, workspaces.value) : null;
  if (resolved) {
    await addWorkspaceChip({ ...resolved, isDirectory: entry.isDirectory });
    return;
  }
  let text = "";
  let isDirectory = Boolean(entry.isDirectory);
  try {
    if (entry.file && !isDirectory) {
      text = decodeTextBytes(new Uint8Array(await entry.file.arrayBuffer()));
    } else if (isDirectory && entry.path) {
      const entries = await backend.listExternalDirectory(entry.path);
      text = entries.map((item) => (item.isDirectory ? `${item.name}/` : item.name)).join("\n");
    } else if (entry.path) {
      const file = await backend.readExternalFile(entry.path);
      text = file.content;
    }
  } catch {
    if (entry.path) {
      try {
        const entries = await backend.listExternalDirectory(entry.path);
        text = entries.map((item) => (item.isDirectory ? `${item.name}/` : item.name)).join("\n");
        isDirectory = true;
      } catch {
        const name = path.split(/[\\/]/).pop() || path;
        error.value = `未能读取 ${name}，已添加空引用`;
      }
    }
  }
  rememberChip({
    id: crypto.randomUUID(),
    workspaceId: "",
    path,
    startLine: 0,
    endLine: isDirectory ? 0 : Math.max(1, text.split(/\r\n|\r|\n/).length),
    text,
    isDirectory,
  });
}

async function onDropWorkspace(file: WorkspaceFileDragPayload) {
  error.value = "";
  await addWorkspaceChip(file);
}

async function onDropOs(entries: OsDroppedEntry[]) {
  error.value = "";
  if (!entries.length) {
    error.value = "没有识别到可引用的文件";
    return;
  }
  for (const entry of entries) await addExternalChip(entry);
}

async function pickPaths(directory: boolean) {
  error.value = "";
  const paths = await backend.selectPaths(directory);
  for (const path of paths) {
    const name = path.split(/[\\/]/).pop() || path;
    await addExternalChip({ path, name, isDirectory: directory });
  }
}

async function pickFiles() {
  await pickPaths(false);
}

async function create() {
  if (!canCreate.value) return;
  busy.value = true;
  error.value = "";
  try {
    const nodes = composer.value?.serialize().nodes ?? [];
    const task = await store.createTask({
      title: form.title.trim(),
      description: formatAgentPrompt(nodes),
      agentId: assigneeId.value,
      projectId: form.projectId || null,
      status: form.status,
    });
    if (task) {
      writeTaskMeta(task.id, { priority: "none", labels: [...form.labels] });
      await store.updateTaskMeta({ taskId: task.id, labels: [...form.labels] });
    }
    form.title = "";
    form.labels = [];
    contexts.value = [];
    composer.value?.clear();
    if (!keepOpen.value) emit("close");
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" @mousedown.self="emit('close')">
    <form class="task-create-dialog" :class="{ 'is-expanded': expanded }" @submit.prevent="create">
      <header>
        <p>
          <template v-if="project">{{ project.name }} <span>›</span> </template>
          手动创建
        </p>
        <div>
          <button type="button" class="task-icon-button" :title="expanded ? '收起' : '展开'" @click="expanded = !expanded">
            <Minimize2 v-if="expanded" :size="15" />
            <Maximize2 v-else :size="15" />
          </button>
          <button type="button" class="task-icon-button" title="关闭" @click="emit('close')"><X :size="16" /></button>
        </div>
      </header>
      <input v-model="form.title" class="task-create-title" placeholder="任务标题" />
      <ChatComposer
        ref="composer"
        embedded
        placeholder="添加描述..."
        :initial-nodes="[]"
        :contexts="contexts"
        :target-options="targetOptions"
        :target-value="form.agentId"
        target-label="Miku"
        target-avatar="M"
        :show-context-usage="false"
        @target-change="form.agentId = $event"
        @drop-workspace-file="onDropWorkspace"
        @drop-os-files="onDropOs"
      >
        <template #tools>
          <div class="task-create-tools">
            <UiSelect v-model="form.status" size="sm" aria-label="待办" :options="statusOptions" />
            <TagPicker
              v-model="form.labels"
              :tags="categories.categories"
              placeholder="标签"
              @create="onTagCreate"
              @manage="showTagManage = true"
            />
            <UiSelect
              v-model="form.projectId"
              clearable
              size="sm"
              aria-label="关联需求"
              placeholder="关联需求"
              :options="projectOptions"
            />
          </div>
        </template>
        <template #actions>
          <UiButton type="button" size="icon" variant="ghost" title="添加文件" @click="pickFiles">
            <Paperclip :size="16" />
          </UiButton>
          <label class="task-create-keep">
            <input v-model="keepOpen" class="ui-checkbox" type="checkbox" />
            继续创建
          </label>
          <UiButton variant="primary" type="submit" :disabled="!canCreate">创建任务</UiButton>
        </template>
      </ChatComposer>
      <p v-if="error" class="task-create-error">{{ error }}</p>
    </form>
    <TagManageDialog
      v-if="showTagManage"
      :tags="categories.categories"
      @close="showTagManage = false"
      @create="createCategory"
      @rename="(id, name) => persistCategories(renameRequirementCategory(categories, id, name))"
      @recolor="(id, color) => persistCategories(recolorRequirementCategory(categories, id, color))"
      @remove="(id) => {
        persistCategories(removeRequirementCategory(categories, id));
        form.labels = form.labels.filter((item) => item !== id);
      }"
    />
  </div>
</template>
