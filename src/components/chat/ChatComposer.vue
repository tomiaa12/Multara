<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import { ArrowUp } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import ChatComposerField from "./ChatComposerField.vue";
import ComposerSuggestMenu, {
  type ComposerSuggestCategoryItem,
  type ComposerSuggestItem,
} from "./ComposerSuggestMenu.vue";
import ContextUsageButton from "./ContextUsageButton.vue";
import type { EditorSelectionContext } from "../../domain/types";
import type { SelectOption } from "../ui/select-types";
import type { ComposerNode } from "./composer-content";
import { useAppStore } from "../../stores/app";
import { getCaretClientRect } from "./composer-caret";
import { filterCommands } from "./composer-mentions";
import { replaceSlashToken } from "./skill-slash";
import { groupChatTargetOptions } from "./chat-target-options";
import {
  activeComposerTrigger,
  replaceTriggerWithReference,
  type ComposerEntityItem,
  type ComposerTrigger,
} from "./composer-entities";
import { t } from "../../i18n";
import {
  isWorkspaceFileDrag,
  listOsDroppedEntries,
  onWorkspaceFileDragChange,
  readWorkspaceFileDragData,
  type OsDroppedEntry,
  type WorkspaceFileDragPayload,
} from "./workspace-file-drag";

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
}>(), {
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
});

const emit = defineEmits<{
  change: [nodes: ComposerNode[]];
  submit: [];
  "paste-image": [files: File[]];
  "target-change": [value: string];
  "drop-workspace-file": [file: WorkspaceFileDragPayload];
  "drop-os-files": [entries: OsDroppedEntry[]];
  "pick-files-or-folders": [];
}>();

const store = useAppStore();
const rootEl = ref<HTMLElement | null>(null);
const dropOver = ref(false);
const suggestTrigger = ref<ComposerTrigger | null>(null);
const suggestMenu = ref<{ handleKeydown: (event: KeyboardEvent) => boolean } | null>(null);
const suggestAnchor = ref<{ left: number; top: number; bottom: number; width?: number } | null>(null);
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
  return {
    id: item.id,
    kind: item.kind,
    label: item.label,
    description: item.description,
    avatar: item.avatar,
    leading,
  };
}

const suggestCategories = computed<ComposerSuggestCategoryItem[]>(() => {
  const data = store.data;
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
    ...commands,
  ];
  const group = (id: string, label: string) => ({
    id,
    label,
    items: entities.filter((item) => item.group === id).map(suggestItem),
  });
  const categories: ComposerSuggestCategoryItem[] = [
    { id: "agent", label: "智能体", groups: [group("agents", "智能体"), group("squads", "小队")] },
    { id: "skill", label: "Skills", groups: [group("skills", "Skills"), group("commands", "会话命令")] },
    { id: "task", label: "任务", groups: [group("tasks", "任务")] },
    { id: "requirement", label: "需求", groups: [group("requirements", "需求")] },
  ];
  if (props.allowFilePicker) {
    categories.push({
      id: "file",
      label: t("tasks.fileOrFolder"),
      groups: [],
      action: "pick-files-or-folders",
    });
  }
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

async function onSuggestSelect(item: ComposerSuggestItem) {
  const trigger = suggestTrigger.value;
  if (!trigger) return;
  const next = item.id.startsWith("command:")
    ? replaceSlashToken(field.value?.snapshot() ?? [], item.label.slice(1))
    : replaceTriggerWithReference(field.value?.snapshot() ?? [], trigger, {
      id: item.id,
      kind: item.kind,
      label: item.label,
      description: item.description,
      avatar: item.avatar,
      group: "",
    });
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
} | null>(null);

const selectedTarget = computed({
  get: () => props.targetValue,
  set: (value: string) => emit("target-change", value),
});
// 按 value 前缀（agent: / squad:）分组展示；小队标签去掉「小队 · 」前缀，由分组标题说明
const targetGroups = computed(() =>
  groupChatTargetOptions(props.targetOptions, { agents: t("nav.agents"), squads: t("nav.squads") }),
);

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
});
</script>

<template>
  <component
    :is="embedded ? 'div' : 'form'"
    ref="rootEl"
    class="chat-composer"
    data-workspace-drop="composer"
    :class="{ 'footer-on-focus': footerMode === 'focus', 'is-drop-over': dropOver, 'is-disabled': disabled }"
    @submit.prevent="emit('submit')"
    @keydown.capture="onComposerKeydown"
    @dragover.capture="onDragOver"
    @dragleave="onDragLeave"
    @drop.capture="onDrop"
  >
    <slot />
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
      @submit="emit('submit')"
      @paste-image="emit('paste-image', $event)"
    />
    <div class="composer-footer">
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
        />
        <template v-else>
          <AgentAvatar :value="targetAvatar" :name="targetLabel" :size="22" />
          <span>{{ targetLabel }}</span>
        </template>
      </label>
      <slot name="tools" />
      <div class="composer-actions">
        <ContextUsageButton v-if="showContextUsage" :placement="usagePlacement" />
        <slot name="actions">
          <UiButton class="ds-send-button send-button" type="submit" :disabled="disabled || !canSend" tip="发送">
            <ArrowUp :size="16" />
          </UiButton>
        </slot>
      </div>
    </div>
  </component>
</template>
