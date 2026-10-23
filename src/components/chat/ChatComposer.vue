<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import { ArrowUp } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import ChatComposerField from "./ChatComposerField.vue";
import ComposerSuggestMenu, { type ComposerSuggestItem } from "./ComposerSuggestMenu.vue";
import ContextUsageButton from "./ContextUsageButton.vue";
import type { EditorSelectionContext } from "../../domain/types";
import type { ComposerNode } from "./composer-content";
import { useAppStore } from "../../stores/app";
import { getCaretClientRect } from "./composer-caret";
import { activeSlashQuery, filterSkillSlash, replaceSlashToken, type SkillSlashHit } from "./skill-slash";
import {
  activeMentionQuery,
  filterCommands,
  filterMentions,
  replaceMentionToken,
  type MentionHit,
} from "./composer-mentions";
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
  targetOptions?: Array<{ value: string; label: string; avatar: string }>;
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
});

const emit = defineEmits<{
  change: [nodes: ComposerNode[]];
  submit: [];
  "paste-image": [files: File[]];
  "target-change": [value: string];
  "drop-workspace-file": [file: WorkspaceFileDragPayload];
  "drop-os-files": [entries: OsDroppedEntry[]];
}>();

const store = useAppStore();
const rootEl = ref<HTMLElement | null>(null);
const dropOver = ref(false);
const slashHits = ref<SkillSlashHit[]>([]);
const slashIndex = ref(0);
const mentionHits = ref<MentionHit[]>([]);
const mentionIndex = ref(0);
const suggestAnchor = ref<{ left: number; top: number; bottom: number; width?: number } | null>(null);
const stopDragWatch = onWorkspaceFileDragChange((state) => {
  dropOver.value = state.over === "composer";
});
onBeforeUnmount(() => {
  stopDragWatch();
});

const suggestKind = computed<"mention" | "slash" | null>(() => {
  if (mentionHits.value.length) return "mention";
  if (slashHits.value.length) return "slash";
  return null;
});
const suggestIndex = computed(() =>
  suggestKind.value === "mention" ? mentionIndex.value : slashIndex.value,
);
const suggestItems = computed<ComposerSuggestItem[]>(() => {
  if (suggestKind.value === "mention") {
    return mentionHits.value.map((hit) => ({
      id: hit.id,
      label: hit.insert.trim() || hit.label,
      description: hit.detail || undefined,
      avatar: hit.avatar,
      leading: hit.kind === "task" ? "hash" : undefined,
    }));
  }
  if (suggestKind.value === "slash") {
    return slashHits.value.map((hit) => ({
      id: hit.id,
      label: `/${hit.name}`,
      description: hit.description || undefined,
      leading: "sparkles",
    }));
  }
  return [];
});

async function refreshSuggestAnchor() {
  await nextTick();
  if (!suggestKind.value) {
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

function onSuggestHighlight(index: number) {
  if (suggestKind.value === "mention") mentionIndex.value = index;
  else if (suggestKind.value === "slash") slashIndex.value = index;
}

function onSuggestSelect(index: number) {
  if (suggestKind.value === "mention") {
    const hit = mentionHits.value[index];
    if (hit) void pickMention(hit);
    return;
  }
  const hit = slashHits.value[index];
  if (hit) void pickSkill(hit);
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
  const line = trailingText(nodes);
  const query = activeSlashQuery(line);
  const skills = query === null ? [] : filterSkillSlash(store.data?.resources ?? [], query);
  const commands = query === null || !props.sessionCommands ? [] : filterCommands(query);
  slashHits.value = [
    ...commands.map((item) => ({ id: item.id, name: item.label, description: item.detail })),
    ...skills,
  ];
  if (slashIndex.value >= slashHits.value.length) slashIndex.value = 0;
  const mention = activeMentionQuery(line);
  mentionHits.value = mention
    ? filterMentions(mention.kind, mention.query, store.data?.agents ?? [], store.data?.tasks ?? [])
    : [];
  if (mentionIndex.value >= mentionHits.value.length) mentionIndex.value = 0;
  void refreshSuggestAnchor();
}

async function pickSkill(hit: SkillSlashHit) {
  const next = replaceSlashToken(field.value?.snapshot() ?? [], hit.name);
  field.value?.restore(next);
  emit("change", next);
  slashHits.value = [];
  mentionHits.value = [];
  await nextTick();
  field.value?.focusEnd();
}

async function pickMention(hit: MentionHit) {
  const next = replaceMentionToken(field.value?.snapshot() ?? [], hit.insert);
  field.value?.restore(next);
  emit("change", next);
  mentionHits.value = [];
  slashHits.value = [];
  await nextTick();
  field.value?.focusEnd();
}

function onComposerKeydown(event: KeyboardEvent) {
  if (!suggestKind.value || !suggestItems.value.length) return;
  const isMention = suggestKind.value === "mention";
  const count = suggestItems.value.length;
  if (event.key === "ArrowDown") {
    event.preventDefault();
    event.stopPropagation();
    if (isMention) mentionIndex.value = (mentionIndex.value + 1) % count;
    else slashIndex.value = (slashIndex.value + 1) % count;
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    event.stopPropagation();
    if (isMention) mentionIndex.value = (mentionIndex.value - 1 + count) % count;
    else slashIndex.value = (slashIndex.value - 1 + count) % count;
    return;
  }
  if (event.key === "Enter" || event.key === "Tab") {
    event.preventDefault();
    event.stopPropagation();
    onSuggestSelect(suggestIndex.value);
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    slashHits.value = [];
    mentionHits.value = [];
    suggestAnchor.value = null;
  }
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
const targetGroups = computed(() => {
  const agents = props.targetOptions.filter((option) => !option.value.startsWith("squad:"));
  const squads = props.targetOptions
    .filter((option) => option.value.startsWith("squad:"))
    .map((option) => ({ ...option, label: option.label.replace(/^小队 · /, "") }));
  return [
    { label: "智能体", options: agents },
    { label: "小队", options: squads },
  ].filter((group) => group.options.length);
});

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
      v-if="suggestItems.length"
      :items="suggestItems"
      :active-index="suggestIndex"
      :anchor="suggestAnchor"
      :aria-label="suggestKind === 'mention' ? 'Mentions' : 'Skills'"
      @highlight="onSuggestHighlight"
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
          <UiButton class="ds-send-button send-button" type="submit" :disabled="disabled || !canSend" title="发送">
            <ArrowUp :size="16" />
          </UiButton>
        </slot>
      </div>
    </div>
  </component>
</template>
