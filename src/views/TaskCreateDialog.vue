<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { Maximize2, Minimize2, Paperclip, X } from "@lucide/vue";
import type { EditorSelectionContext, MessageAttachmentInput, TaskStatus } from "../domain/types";
import { isMiku } from "../domain/types";
import { useAppStore } from "../stores/app";
import * as backend from "../services/backend";
import { fileToBase64 } from "../utils/file-base64";
import ChatComposer from "../components/chat/ChatComposer.vue";
import ComposerPasteImages from "../components/chat/ComposerPasteImages.vue";
import { buildChatTargetOptions } from "../components/chat/chat-target-options";
import { agentRuntime, modeSupported, modeUnsupportedReason } from "../components/chat/run-mode";
import {
  formatAgentPrompt,
  type ComposerNode,
} from "../components/chat/composer-content";
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
  recolorRequirementCategory,
  removeRequirementCategory,
  renameRequirementCategory,
  saveRequirementCategories,
  sharedRequirementCategories,
  type RequirementCategoryState,
} from "./dashboard/requirement-categories";
import { TASK_COLUMNS } from "./task-board";
import { writeTaskMeta } from "./task-meta";
import { requirementIds, syncRequirementChips } from "./task-project-links";
import { escapeClosesDialog } from "./dialog-escape";
import { t } from "../i18n";

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
const categories = sharedRequirementCategories();
const contexts = ref<EditorSelectionContext[]>([]);
const pastes = ref<Array<MessageAttachmentInput & { previewUrl: string }>>([]);
const composer = ref<{
  clear: () => void;
  insertChip: (item: EditorSelectionContext) => void;
  focus: () => void;
  restore: (nodes: ComposerNode[]) => void;
  snapshot: () => ComposerNode[];
  serialize: () => { text: string; nodes: ComposerNode[]; empty: boolean };
} | null>(null);
let syncingProjectChips = false;

const PASTE_LIMIT = 8;
const PASTE_MAX_BYTES = 25 * 1024 * 1024;

const form = reactive({
  title: "",
  status: props.initialStatus,
  agentId: "",
  projectIds: [] as string[],
  labels: [] as string[],
});

const agents = computed(() => store.data?.agents ?? []);
const squads = computed(() => store.data?.squads ?? []);
const projects = computed(() => store.data?.projects ?? []);
const project = computed(() => projects.value.find((item) => item.id === form.projectIds[0]));
const workspaces = computed(() => {
  const all = store.data?.workspaces ?? [];
  if (!form.projectIds.length) return all;
  return all.filter((item) => form.projectIds.includes(item.projectId));
});
const assigneeId = computed(() => {
  if (form.agentId.startsWith("squad:")) {
    const squadId = form.agentId.slice("squad:".length);
    return squads.value.find((item) => item.id === squadId)?.leaderAgentId ?? "";
  }
  return form.agentId.replace(/^agent:/, "");
});
const targetOptions = computed(() =>
  buildChatTargetOptions(
    agents.value,
    squads.value,
    (name) => t("chat.squadLabel", { name }),
  ),
);
/** 创建入口只给待规划 / 待办；开跑后系统会同步到进行中等后续状态。 */
const CREATE_STATUS_IDS = new Set(["backlog", "todo"]);
const statusOptions = computed(() =>
  TASK_COLUMNS
    .filter((column) => CREATE_STATUS_IDS.has(column.id))
    .map((column) => ({ value: column.id, label: column.label })),
);
const projectOptions = computed(() => projects.value.map((item) => ({ value: item.id, label: item.name })));
const canCreate = computed(() => Boolean(form.title.trim() && assigneeId.value && !busy.value));
const planFirst = ref(false);
/** 小队按队长的运行时判断能否先出计划 */
const assigneeRuntime = computed(() => agentRuntime(store.data, assigneeId.value));
const planSupported = computed(() => modeSupported(assigneeRuntime.value, "plan"));
const requestedMode = computed(() => (planFirst.value && planSupported.value ? "plan" : "auto"));

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

function sameIds(left: string[], right: string[]) {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

function onComposerChange(nodes: ComposerNode[]) {
  if (syncingProjectChips) return;
  const ids = requirementIds(nodes);
  if (!sameIds(ids, form.projectIds)) form.projectIds = ids;
}

watch(
  () => [...form.projectIds],
  (ids) => {
    const current = composer.value?.snapshot() ?? [];
    if (sameIds(requirementIds(current), ids)) return;
    syncingProjectChips = true;
    composer.value?.restore(syncRequirementChips(current, ids, projects.value));
    syncingProjectChips = false;
  },
);

function hasInput() {
  return Boolean(
    form.title.trim()
    || form.labels.length
    || form.projectIds.length
    || pastes.value.length
    || composer.value?.serialize().empty === false,
  );
}

function onKey(event: KeyboardEvent) {
  if (!escapeClosesDialog(event)) return;
  if (hasInput() && !window.confirm(t("common.discardUnsavedConfirm"))) return;
  emit("close");
}

function clearPastes() {
  for (const image of pastes.value) URL.revokeObjectURL(image.previewUrl);
  pastes.value = [];
}

onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKey);
  clearPastes();
});

function rememberChip(context: EditorSelectionContext) {
  contexts.value = [...contexts.value, context];
  composer.value?.insertChip(context);
  composer.value?.focus();
}

async function pasteImages(files: File[]) {
  error.value = "";
  const room = Math.max(0, PASTE_LIMIT - pastes.value.length);
  for (const file of files.slice(0, room)) {
    try {
      if (file.size > PASTE_MAX_BYTES) {
        error.value = t("chat.pasteTooLarge");
        continue;
      }
      const saved = await backend.savePastedAttachment(await fileToBase64(file), file.type || "image/png", file.name || undefined);
      pastes.value.push({ ...saved, previewUrl: URL.createObjectURL(file) });
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : t("chat.pasteFailed");
    }
  }
}

function removePaste(path: string) {
  const target = pastes.value.find((item) => item.path === path);
  if (target) URL.revokeObjectURL(target.previewUrl);
  pastes.value = pastes.value.filter((item) => item.path !== path);
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
    error.value = t("chat.attachEmpty", { name });
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
  const path = entry.path || entry.name || t("chat.externalFile");
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
        error.value = t("chat.attachEmpty", { name });
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
    error.value = t("chat.noRefFiles");
    return;
  }
  const images = entries.flatMap((entry) =>
    entry.file?.type.startsWith("image/") ? [entry.file] : [],
  );
  if (images.length) await pasteImages(images);
  for (const entry of entries) {
    if (entry.file?.type.startsWith("image/")) continue;
    await addExternalChip(entry);
  }
}

async function pickFilesOrFolders() {
  error.value = "";
  const picked = await backend.selectFilesOrFolders();
  if (!picked.length) return;
  for (const item of picked) {
    const name = item.path.split(/[\\/]/).pop() || item.path;
    await addExternalChip({ path: item.path, name, isDirectory: item.isDirectory });
  }
}

async function create() {
  if (!canCreate.value) return;
  busy.value = true;
  error.value = "";
  try {
    const nodes = composer.value?.serialize().nodes ?? [];
    const description = formatAgentPrompt(nodes);
    const attachments = pastes.value.map(({ name, path, mimeType, size }) => ({ name, path, mimeType, size }));
    // 有粘贴附件时先建为 backlog，再 send 带附件开跑，避免 todo 创建时无附件先排队。
    const createStatus = attachments.length && form.status === "todo" ? "backlog" : form.status;
    const task = await store.createTask({
      title: form.title.trim(),
      description,
      agentId: assigneeId.value,
      projectId: form.projectIds[0] ?? null,
      projectIds: [...form.projectIds],
      status: createStatus,
      requestedMode: requestedMode.value,
    });
    if (task) {
      writeTaskMeta(task.id, { priority: "none", labels: [...form.labels] });
      await store.updateTaskMeta({ taskId: task.id, labels: [...form.labels] });
      if (attachments.length) {
        store.send(description || form.title.trim(), task.sessionId, attachments, { mode: requestedMode.value });
      }
    }
    form.title = "";
    form.labels = [];
    form.projectIds = [];
    contexts.value = [];
    clearPastes();
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
  <UiModal @mousedown.self="emit('close')">
    <form class="task-create-dialog" :class="{ 'is-expanded': expanded }" @submit.prevent="create">
      <header>
        <p>
          <template v-if="project">{{ project.name }} <span>›</span> </template>
          {{ t('skills.create.method.manual') }}
        </p>
        <div>
          <UiTooltip :content="expanded ? t('skills.collapse') : t('skills.expand')"><button type="button" class="task-icon-button" @click="expanded = !expanded">
            <Minimize2 v-if="expanded" :size="15" />
            <Maximize2 v-else :size="15" />
          </button></UiTooltip>
          <UiTooltip :content="t('common.close')"><button type="button" class="task-icon-button" @click="emit('close')"><X :size="16" /></button></UiTooltip>
        </div>
      </header>
      <input v-model="form.title" class="task-create-title" :placeholder="t('tasks.titlePlaceholder')" />
      <ChatComposer
        ref="composer"
        embedded
        dictation
        :placeholder="t('tasks.addDescription')"
        :initial-nodes="[]"
        :contexts="contexts"
        :target-options="targetOptions"
        :target-value="form.agentId"
        target-label="Miku"
        target-avatar="M"
        :show-context-usage="false"
        allow-file-picker
        @change="onComposerChange"
        @target-change="form.agentId = $event"
        @drop-workspace-file="onDropWorkspace"
        @drop-os-files="onDropOs"
        @paste-image="pasteImages"
        @pick-files-or-folders="pickFilesOrFolders"
      >
        <ComposerPasteImages
          :images="pastes.map((image) => ({ key: image.path, src: image.previewUrl, name: image.name }))"
          removable
          @remove="removePaste"
        />
        <template #tools>
          <div class="task-create-tools">
            <UiSelect v-model="form.status" size="sm" :aria-label="t('tasks.field.status')" :options="statusOptions" />
            <TagPicker
              v-model="form.labels"
              :tags="categories.categories"
              :placeholder="t('tasks.tags')"
              @create="onTagCreate"
              @manage="showTagManage = true"
            />
            <UiSelect
              v-model="form.projectIds"
              multiple
              searchable
              :page-size="5"
              clearable
              size="sm"
              :aria-label="t('tasks.linkProject')"
              :placeholder="t('tasks.linkProject')"
              :options="projectOptions"
            />
          </div>
        </template>
        <template #actions>
          <UiButton type="button" size="icon" variant="ghost" :tip="t('tasks.addFileOrFolder')" @click="pickFilesOrFolders">
            <Paperclip :size="16" />
          </UiButton>
          <UiTooltip :content="planSupported ? t('tasks.planFirstHint') : modeUnsupportedReason(assigneeRuntime)">
            <label class="task-create-keep" :class="{ 'is-disabled': !planSupported }">
              <input v-model="planFirst" class="ui-checkbox" type="checkbox" :disabled="!planSupported" />
              {{ t('tasks.planFirst') }}
            </label>
          </UiTooltip>
          <label class="task-create-keep">
            <input v-model="keepOpen" class="ui-checkbox" type="checkbox" />
            {{ t('tasks.keepOpen') }}
          </label>
          <UiButton variant="primary" type="submit" :disabled="!canCreate">{{ t('tasks.createSubmit') }}</UiButton>
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
  </UiModal>
</template>
