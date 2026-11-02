<script setup lang="ts">
import { computed, inject, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { ArrowUp } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import MicButton from "../dictation/MicButton.vue";
import { dictationService } from "../../dictation/dictation-service";
import { useDictationStore } from "../../stores/dictation";
import {
  listComposerRecent,
  type ComposerRecentPlace,
} from "../../stores/composer-recent-paths";
import * as backend from "../../services/backend";
import ChatComposerField from "./ChatComposerField.vue";
import ComposerSuggestMenu, {
  type ComposerSuggestCategoryItem,
  type ComposerSuggestGroup,
  type ComposerSuggestItem,
} from "./ComposerSuggestMenu.vue";
import ContextUsageButton from "./ContextUsageButton.vue";
import ChatModePicker from "./ChatModePicker.vue";
import ComposerPlanHint from "./ComposerPlanHint.vue";
import type { EditorSelectionContext } from "../../domain/types";
import type { SelectOption } from "../ui/select-types";
import type { ComposerNode } from "./composer-content";
import { useAppStore } from "../../stores/app";
import { getCaretClientRect } from "./composer-caret";
import { filterCommands } from "./composer-mentions";
import { replaceSlashToken } from "./skill-slash";
import { groupChatTargetOptions, type ChatTargetOption } from "./chat-target-options";
import ProviderLogo from "../ProviderLogo.vue";
import {
  activeComposerTrigger,
  removeComposerTrigger,
  replaceTriggerWithReference,
  type ComposerEntityItem,
  type ComposerSuggestCategoryId,
  type ComposerTrigger,
} from "./composer-entities";
import type { ComposerEntityKind } from "./composer-content";
import {
  SYSTEM_PICK_ID,
  composerOpenEditorsKey,
  fileSuggestId,
  parseFileSuggestId,
  pathFileName,
  type ComposerAttachOpenFile,
} from "./composer-attach-sources";
import { t } from "../../i18n";
import {
  isWorkspaceFileDrag,
  listOsDroppedEntries,
  onWorkspaceFileDragChange,
  readWorkspaceFileDragData,
  type OsDroppedEntry,
  type WorkspaceFileDragPayload,
} from "./workspace-file-drag";

const ROOT_ENTRY_CAP = 200;

const props = withDefaults(defineProps<{
  placeholder: string;
  initialNodes: ComposerNode[];
  contexts: EditorSelectionContext[];
  targetLabel: string;
  targetAvatar: string;
  targetValue?: string;
  targetOptions?: SelectOption<string>[];
  allowTargetChange?: boolean;
  footerMode?: "always" | "focus";
  canSend?: boolean;
  disabled?: boolean;
  usagePlacement?: "up" | "down";
  showContextUsage?: boolean;
  /** 嵌在别的表单里时用 div，避免再套一层 form。 */
  embedded?: boolean;
  /** 是否展示 /compact、/fork 等会话命令（需求工作台 / 右下角气泡）。 */
  sessionCommands?: boolean;
  /** 是否在级联一级菜单展示系统文件/文件夹选择入口。 */
  allowFilePicker?: boolean;
  /** 最近打开隔离场景；工作台再按 projectId 分桶。 */
  recentPlace?: ComposerRecentPlace;
  /** 关联项目；用于工作区根列表与 workbench 最近隔离。 */
  projectId?: string | null;
  /** 传入会话 id 时显示模式选择与计划提示行（只读 / 预览场景不传）。 */
  modeSessionId?: string;
  /** 主输入框开启系统听写；历史气泡等不要传。 */
  dictation?: boolean;
}>(), {
  modeSessionId: "",
  targetValue: "",
  targetOptions: () => [],
  allowTargetChange: true,
  footerMode: "always",
  canSend: true,
  disabled: false,
  usagePlacement: "up",
  showContextUsage: true,
  embedded: false,
  sessionCommands: false,
  allowFilePicker: false,
  recentPlace: "chat",
  projectId: null,
  dictation: false,
});

const emit = defineEmits<{
  change: [nodes: ComposerNode[]];
  submit: [];
  "paste-image": [files: File[]];
  "target-change": [value: string];
  "drop-workspace-file": [file: WorkspaceFileDragPayload];
  "drop-os-files": [entries: OsDroppedEntry[]];
  "pick-files-or-folders": [];
  "attach-file-context": [file: ComposerAttachOpenFile];
}>();

const store = useAppStore();
const dictationStore = useDictationStore();
const dictationTargetId = `composer-${crypto.randomUUID()}`;
const rootEl = ref<HTMLElement | null>(null);
const showDictation = computed(() => props.dictation && dictationStore.visible && !props.disabled);
const dropOver = ref(false);
const suggestTrigger = ref<ComposerTrigger | null>(null);
const suggestMenu = ref<{ handleKeydown: (event: KeyboardEvent) => boolean } | null>(null);
const suggestAnchor = ref<{ left: number; top: number; bottom: number; width?: number } | null>(null);
const workspaceRootItems = ref<ComposerSuggestItem[]>([]);
const injectedOpenEditors = inject(composerOpenEditorsKey, null);
const stopDragWatch = onWorkspaceFileDragChange((state) => {
  dropOver.value = state.over === "composer";
});
onBeforeUnmount(() => {
  stopDragWatch();
});

function suggestItem(item: ComposerEntityItem): ComposerSuggestItem {
  let leading: ComposerSuggestItem["leading"] = "bot";
  if (item.kind === "skill") leading = "sparkles";
  if (item.kind === "task") leading = "hash";
  if (item.kind === "requirement") leading = "requirement";
  if (item.kind === "session") leading = "session";
  return {
    id: item.id,
    kind: item.kind,
    label: item.label,
    description: item.description,
    avatar: item.avatar,
    leading,
  };
}

function fileSuggestItem(file: {
  workspaceId: string;
  path: string;
  isDirectory?: boolean;
  description?: string;
}): ComposerSuggestItem {
  const isDirectory = Boolean(file.isDirectory);
  return {
    id: fileSuggestId(file.workspaceId, file.path),
    kind: "file",
    label: pathFileName(file.path),
    description: file.description ?? file.path,
    leading: isDirectory ? "folder" : "file",
    isDirectory,
  };
}

function buildFileGroups(): ComposerSuggestGroup[] {
  const groups: ComposerSuggestGroup[] = [
    {
      id: "system",
      label: "",
      items: [{
        id: SYSTEM_PICK_ID,
        kind: "file",
        label: t("chat.context.pickFromSystem"),
        leading: "picker",
      }],
    },
  ];
  const recent = listComposerRecent(props.recentPlace, props.projectId).map((item) =>
    fileSuggestItem({
      workspaceId: item.workspaceId,
      path: item.path,
      isDirectory: item.isDirectory,
      description: item.path,
    }),
  );
  if (recent.length) {
    groups.push({ id: "recent", label: t("chat.context.recentFiles"), items: recent });
  }
  const openEditors = (injectedOpenEditors?.value ?? []).map((item) =>
    fileSuggestItem({
      workspaceId: item.workspaceId,
      path: item.path,
      isDirectory: item.isDirectory,
    }),
  );
  if (openEditors.length) {
    groups.push({ id: "open", label: t("chat.context.openEditors"), items: openEditors });
  }
  if (workspaceRootItems.value.length) {
    groups.push({
      id: "workspace",
      label: t("chat.context.workspaceFiles"),
      items: workspaceRootItems.value,
    });
  }
  return groups;
}

async function refreshWorkspaceRoots() {
  if (!props.allowFilePicker || !props.projectId) {
    workspaceRootItems.value = [];
    return;
  }
  const workspaces = (store.data?.workspaces ?? []).filter((item) => item.projectId === props.projectId);
  if (!workspaces.length) {
    workspaceRootItems.value = [];
    return;
  }
  const multi = workspaces.length > 1;
  const items: ComposerSuggestItem[] = [];
  for (const workspace of workspaces) {
    if (items.length >= ROOT_ENTRY_CAP) break;
    try {
      const entries = await backend.listWorkspaceDirectory(workspace.id);
      for (const entry of entries) {
        if (items.length >= ROOT_ENTRY_CAP) break;
        items.push(fileSuggestItem({
          workspaceId: workspace.id,
          path: entry.path,
          isDirectory: entry.isDirectory,
          description: multi ? `${workspace.name} · ${entry.path}` : entry.path,
        }));
      }
    } catch {
      // 单个工作区列失败时跳过，其它继续
    }
  }
  workspaceRootItems.value = items;
}

watch(
  () => [suggestTrigger.value, props.projectId, props.allowFilePicker] as const,
  ([trigger]) => {
    if (trigger && props.allowFilePicker) void refreshWorkspaceRoots();
  },
);

const suggestCategories = computed<ComposerSuggestCategoryItem[]>(() => {
  const data = store.data;
  const currentSessionId = props.modeSessionId;
  const commands = props.sessionCommands
    ? filterCommands("").map((item) => ({
      id: `command:${item.id}`,
      kind: "skill" as const,
      label: `/${item.label}`,
      description: item.detail,
      group: "commands",
    }))
    : [];
  const entities: ComposerEntityItem[] = [
    ...(data?.agents ?? []).map((item) => ({
      id: item.id,
      kind: "agent" as const,
      label: item.name,
      description: item.description || item.model,
      avatar: item.avatar,
      group: "agents",
    })),
    ...(data?.squads ?? []).map((item) => ({
      id: item.id,
      kind: "squad" as const,
      label: item.name,
      description: item.description,
      avatar: data?.agents.find((agent) => agent.id === item.leaderAgentId)?.avatar,
      group: "squads",
    })),
    ...(data?.resources ?? [])
      .filter((item) => item.kind === "skill" && item.enabled)
      .map((item) => ({
        id: item.id,
        kind: "skill" as const,
        label: item.name,
        description: item.description,
        group: "skills",
      })),
    ...(data?.tasks ?? []).map((item) => ({
      id: item.id,
      kind: "task" as const,
      label: item.title,
      description: item.status,
      group: "tasks",
    })),
    ...(data?.projects ?? []).map((item) => ({
      id: item.id,
      kind: "requirement" as const,
      label: item.name,
      description: item.note,
      group: "requirements",
    })),
    ...(data?.sessions ?? [])
      .filter((item) => item.id !== currentSessionId)
      .map((item) => ({
        id: item.id,
        kind: "session" as const,
        label: item.title || t("chat.untitledSession"),
        description: item.targetType === "squad" ? t("nav.squads") : t("nav.agents"),
        group: "sessions",
      })),
    ...commands,
  ];
  const group = (id: string, label: string) => ({
    id,
    label,
    items: entities.filter((item) => item.group === id).map(suggestItem),
  });
  // 一级顺序对齐 VS Code Attach Context：文件 / 会话 / Issue / PR，再是原有 @ 分类
  const categories: ComposerSuggestCategoryItem[] = [];
  if (props.allowFilePicker) {
    categories.push({
      id: "file",
      label: t("chat.context.files"),
      groups: buildFileGroups(),
      emptyText: t("chat.context.filesEmpty"),
    });
  }
  categories.push(
    {
      id: "session",
      label: t("chat.context.sessions"),
      groups: [group("sessions", t("chat.context.sessions"))],
      emptyText: t("chat.context.sessionsEmpty"),
    },
    {
      id: "issue",
      label: t("chat.context.issues"),
      groups: [],
      emptyText: t("chat.context.issuesEmpty"),
    },
    {
      id: "pullRequest",
      label: t("chat.context.pullRequests"),
      groups: [],
      emptyText: t("chat.context.pullRequestsEmpty"),
    },
    { id: "agent", label: t("chat.context.agents"), groups: [group("agents", t("nav.agents")), group("squads", t("nav.squads"))] },
    { id: "skill", label: t("chat.context.skills"), groups: [group("skills", "Skills"), group("commands", t("chat.context.commands"))] },
    { id: "task", label: t("chat.context.tasks"), groups: [group("tasks", t("chat.context.tasks"))] },
    { id: "requirement", label: t("chat.context.requirements"), groups: [group("requirements", t("chat.context.requirements"))] },
  );
  return categories;
});

async function refreshSuggestAnchor() {
  await nextTick();
  if (!suggestTrigger.value) {
    suggestAnchor.value = null;
    return;
  }
  const caret = getCaretClientRect();
  if (caret) {
    suggestAnchor.value = {
      left: caret.left,
      top: caret.top,
      bottom: caret.bottom,
      width: Math.max(caret.width, 160),
    };
    return;
  }
  // 光标矩形拿不到时，贴着输入区底部展开，避免浮层落到 (0,0)
  const host = rootEl.value;
  const root = host instanceof HTMLElement
    ? host
    : (host as { $el?: HTMLElement } | null)?.$el ?? null;
  const fieldEl = root?.querySelector<HTMLElement>(".composer-field");
  const rect = fieldEl?.getBoundingClientRect();
  suggestAnchor.value = rect
    ? { left: rect.left, top: rect.top, bottom: rect.bottom, width: Math.min(rect.width, 280) }
    : null;
}

function onDragOver(event: DragEvent) {
  if (props.disabled) return;
  if (!isWorkspaceFileDrag(event.dataTransfer)) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  dropOver.value = true;
}

function onDragLeave(event: DragEvent) {
  const current = event.currentTarget as HTMLElement | null;
  const next = event.relatedTarget as Node | null;
  if (current && next && current.contains(next)) return;
  dropOver.value = false;
}

function onDrop(event: DragEvent) {
  dropOver.value = false;
  if (props.disabled) {
    event.preventDefault();
    return;
  }
  const fromTree = readWorkspaceFileDragData(event.dataTransfer);
  if (fromTree) {
    event.preventDefault();
    event.stopPropagation();
    emit("drop-workspace-file", fromTree);
    return;
  }
  // 与 dragover 同一判定，避免只认 files.length 而漏掉 uri-list / 文件夹
  if (!isWorkspaceFileDrag(event.dataTransfer)) return;
  event.preventDefault();
  event.stopPropagation();
  // dataTransfer 在 handler 返回后可能被清空；先快照。
  // beginExternalDrop 会在下一帧还原 contenteditable（清掉系统默认插入），
  // 必须等还原后再通知父组件插 chip，否则 chip 会被一并清掉。
  const entries = listOsDroppedEntries(event.dataTransfer);
  field.value?.beginExternalDrop();
  requestAnimationFrame(() => {
    emit("drop-os-files", entries);
  });
}

function trailingText(nodes: ComposerNode[]) {
  const text = nodes.map((node) => (node.type === "text" ? node.value : " ")).join("");
  return text.split("\n").at(-1) ?? "";
}

function onComposerChange(nodes: ComposerNode[]) {
  emit("change", nodes);
  suggestTrigger.value = activeComposerTrigger(trailingText(nodes));
  void refreshSuggestAnchor();
}

const ENTITY_KINDS = new Set<ComposerEntityKind>([
  "agent", "squad", "skill", "task", "requirement", "session",
]);

function stripSuggestTrigger() {
  const trigger = suggestTrigger.value;
  if (!trigger) return;
  // + 菜单假触发没有落在文本里；只有真正打了 @/#/ 才需要剥掉
  const nodes = field.value?.snapshot() ?? [];
  const next = removeComposerTrigger(nodes, trigger);
  if (next === nodes) return;
  field.value?.restore(next);
  emit("change", next);
}

async function onSuggestSelect(item: ComposerSuggestItem) {
  const trigger = suggestTrigger.value;
  if (!trigger) return;
  if (item.id === SYSTEM_PICK_ID) {
    stripSuggestTrigger();
    closeSuggest();
    emit("pick-files-or-folders");
    return;
  }
  if (item.kind === "file") {
    const parsed = parseFileSuggestId(item.id);
    if (!parsed) return;
    stripSuggestTrigger();
    closeSuggest();
    emit("attach-file-context", {
      workspaceId: parsed.workspaceId,
      path: parsed.path,
      isDirectory: item.isDirectory,
    });
    await nextTick();
    field.value?.focusEnd();
    return;
  }
  if (!ENTITY_KINDS.has(item.kind as ComposerEntityKind)) return;
  const entity: ComposerEntityItem = {
    id: item.id,
    kind: item.kind as ComposerEntityKind,
    label: item.label,
    description: item.description,
    avatar: item.avatar,
    group: "",
  };
  const next = item.id.startsWith("command:")
    ? replaceSlashToken(field.value?.snapshot() ?? [], item.label.slice(1))
    : replaceTriggerWithReference(field.value?.snapshot() ?? [], trigger, entity);
  field.value?.restore(next);
  emit("change", next);
  closeSuggest();
  await nextTick();
  field.value?.focusEnd();
}

function closeSuggest() {
  suggestTrigger.value = null;
  suggestAnchor.value = null;
}

function onSuggestCategoryAction(action: "pick-files-or-folders") {
  closeSuggest();
  if (action === "pick-files-or-folders") emit("pick-files-or-folders");
}

/** 发送旁 + 打开与 @ 相同的引用面板；再点一次关闭（对齐 VS Code） */
function openAttachMenu(defaultCategory: ComposerSuggestCategoryId = "file") {
  if (props.disabled) return;
  if (suggestTrigger.value) {
    closeSuggest();
    return;
  }
  const category = suggestCategories.value.some((item) => item.id === defaultCategory)
    ? defaultCategory
    : (suggestCategories.value[0]?.id ?? "agent");
  suggestTrigger.value = { symbol: "@", category, query: "" };
  void refreshSuggestAnchor();
}

function onComposerKeydown(event: KeyboardEvent) {
  if (!suggestTrigger.value) return;
  suggestMenu.value?.handleKeydown(event);
}

const field = ref<{
  focus: () => void;
  focusEnd: () => void;
  clear: () => void;
  insertChip: (item: EditorSelectionContext) => void;
  insertText: (value: string) => void;
  restore: (nodes: ComposerNode[]) => void;
  snapshot: () => ComposerNode[];
  serialize: () => { text: string; nodes: ComposerNode[]; empty: boolean };
  isEmpty: () => boolean;
  beginExternalDrop: () => void;
  getEditorEl: () => HTMLElement | null;
  notifyChange: () => void;
  isComposing: () => boolean;
} | null>(null);

async function onSubmit() {
  if (showDictation.value) {
    await dictationService.stopBeforeSubmit(dictationTargetId);
  }
  emit("submit");
}

const selectedTarget = computed({
  get: () => props.targetValue,
  set: (value: string) => emit("target-change", value),
});
// 按 value 前缀（agent: / squad:）分组展示；小队标签去掉「小队 · 」前缀，由分组标题说明
const targetGroups = computed(() =>
  groupChatTargetOptions(props.targetOptions, { agents: t("nav.agents"), squads: t("nav.squads") }),
);

function isAgentTargetOption(option: SelectOption | null | undefined): option is ChatTargetOption {
  return Boolean(option && String(option.value).startsWith("agent:"));
}

defineExpose({
  focus: () => field.value?.focus(),
  focusEnd: () => field.value?.focusEnd(),
  clear: () => field.value?.clear(),
  insertChip: (item: EditorSelectionContext) => field.value?.insertChip(item),
  insertText: (value: string) => field.value?.insertText(value),
  restore: (nodes: ComposerNode[]) => field.value?.restore(nodes),
  snapshot: () => field.value?.snapshot() ?? [],
  serialize: () =>
    field.value?.serialize() ?? { text: "", nodes: [] as ComposerNode[], empty: true },
  isEmpty: () => field.value?.isEmpty() ?? true,
  openAttachMenu,
});
</script>

<template>
  <component
    :is="embedded ? 'div' : 'form'"
    ref="rootEl"
    class="chat-composer"
    data-workspace-drop="composer"
    :class="{ 'footer-on-focus': footerMode === 'focus', 'is-drop-over': dropOver, 'is-disabled': disabled }"
    @submit.prevent="onSubmit"
    @keydown.capture="onComposerKeydown"
    @dragover.capture="onDragOver"
    @dragleave="onDragLeave"
    @drop.capture="onDrop"
  >
    <slot />
    <ComposerPlanHint v-if="modeSessionId" :session-id="modeSessionId" />
    <div v-if="dropOver" class="composer-drop-overlay" aria-hidden="true">
      <span>{{ t("chat.dropToAttach") }}</span>
    </div>
    <ComposerSuggestMenu
      v-if="suggestTrigger"
      ref="suggestMenu"
      :categories="suggestCategories"
      :default-category="suggestTrigger.category"
      :query="suggestTrigger.query"
      :anchor="suggestAnchor"
      aria-label="插入引用"
      @category-action="onSuggestCategoryAction"
      @close="closeSuggest"
      @select="onSuggestSelect"
    />
    <ChatComposerField
      ref="field"
      :placeholder="placeholder"
      :initial-nodes="initialNodes"
      :contexts="contexts"
      :disabled="disabled"
      @change="onComposerChange"
      @submit="onSubmit"
      @paste-image="emit('paste-image', $event)"
    />
    <div class="composer-footer">
      <div class="composer-footer-start">
        <label class="target-picker">
          <UiSelect
            v-if="allowTargetChange && targetOptions.length"
            v-model="selectedTarget"
            variant="ghost"
            size="sm"
            searchable
            :page-size="5"
            aria-label="选择要询问的智能体或小队"
            :groups="targetGroups"
          >
            <template #option-description="{ option, parts }">
              <span
                v-if="isAgentTargetOption(option)"
                class="target-option-runtime"
              >
                <ProviderLogo
                  v-if="option.runtimeProvider"
                  class="target-option-runtime-logo"
                  :provider="option.runtimeProvider"
                  :size="12"
                />
                <span v-if="option.runtimeName" class="target-option-runtime-name">{{ option.runtimeName }}</span>
                <span class="target-option-runtime-model">{{ option.runtimeModel || "auto" }}</span>
                <span v-if="option.agentDescription" class="target-option-runtime-desc">{{ option.agentDescription }}</span>
              </span>
              <template v-else-if="parts">{{ parts[0] }}<mark v-if="parts[1]">{{ parts[1] }}</mark>{{ parts[2] }}</template>
            </template>
          </UiSelect>
          <template v-else>
            <AgentAvatar :value="targetAvatar" :name="targetLabel" :size="22" />
            <span>{{ targetLabel }}</span>
          </template>
        </label>
        <ChatModePicker v-if="modeSessionId" :session-id="modeSessionId" />
      </div>
      <slot name="tools" />
      <div class="composer-actions">
        <ContextUsageButton v-if="showContextUsage" :placement="usagePlacement" />
        <MicButton
          v-if="showDictation"
          :target-id="dictationTargetId"
          :get-editor="() => field?.getEditorEl() ?? null"
          :notify-change="() => field?.notifyChange()"
          :is-composing="() => field?.isComposing() ?? false"
        />
        <slot name="actions">
          <UiButton class="ds-send-button send-button" type="submit" :disabled="disabled || !canSend" tip="发送">
            <ArrowUp :size="16" />
          </UiButton>
        </slot>
      </div>
    </div>
  </component>
</template>
