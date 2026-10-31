<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { ArrowUp, FileText, Square } from "@lucide/vue";
import ChatComposer from "./ChatComposer.vue";
import ComposerPasteImages from "./ComposerPasteImages.vue";
import type { SelectOption } from "../ui/select-types";
import type {
  ChatMessage,
  EditorSelectionContext,
  MessageAttachment,
  MessageAttachmentInput,
} from "../../domain/types";
import * as backend from "../../services/backend";
import { watchOffscreen } from "../../ui/offscreen-pause";
import type { ComposerNode } from "./composer-content";
import { formatAgentPrompt, isFileChip, isPastedImage, parseUserPromptNodes } from "./composer-content";
import { DEFAULT_RUN_MODE, modeLabel } from "./run-mode";
import { t } from "../../i18n";

const props = withDefaults(defineProps<{
  message: ChatMessage;
  targetLabel?: string;
  targetAvatar?: string;
  targetValue?: string;
  targetOptions?: SelectOption<string>[];
  allowTargetChange?: boolean;
  enableReplay?: boolean;
  viewOnly?: boolean;
  restoreNonce?: number;
  restoreMessageId?: string;
  running?: boolean;
}>(), {
  targetLabel: "智能体",
  targetAvatar: "M",
  targetValue: "",
  targetOptions: () => [],
  allowTargetChange: true,
  enableReplay: true,
  viewOnly: false,
  restoreNonce: 0,
  restoreMessageId: "",
  running: false,
});
const emit = defineEmits<{
  "start-reask": [messageId: string, content: string, attachments: MessageAttachmentInput[]];
  "target-change": [value: string];
  stop: [];
}>();
const previews = ref<Record<string, string>>({});
const editNotice = ref("");
const composer = ref<{
  focus: () => void;
  focusEnd: () => void;
  restore: (nodes: ComposerNode[]) => void;
  serialize: () => { text: string; nodes: ComposerNode[]; empty: boolean };
} | null>(null);
const expanded = ref(false);
const draftEmpty = ref(false);

const nodes = computed(() => parseUserPromptNodes(props.message.content));
const pastedImages = computed(() => props.message.attachments.filter(isPastedImage));
const files = computed(() => props.message.attachments.filter((item) => !isPastedImage(item)));
const contexts = computed<EditorSelectionContext[]>(() =>
  nodes.value
    .filter(isFileChip)
    .map((node) => ({
      id: node.id,
      workspaceId: "",
      path: node.path,
      startLine: node.startLine,
      endLine: node.endLine,
      text: node.text,
      isDirectory: node.isDirectory,
    })),
);
/** 计划操作产生的消息标「批准计划 / 完善计划」；其余只标非自动模式 */
const promptTag = computed(() => {
  const planAction = props.message.sourceMeta?.planAction;
  if (planAction === "approve") return { kind: "approve", label: t("chat.plan.userApprove") };
  if (planAction === "refine") return { kind: "refine", label: t("chat.plan.userRefine") };
  const mode = props.message.requestedMode;
  if (!mode || mode === DEFAULT_RUN_MODE) return null;
  return { kind: mode, label: modeLabel(mode) };
});
const replayAttachments = computed<MessageAttachmentInput[]>(() =>
  props.message.attachments.map(({ name, path, mimeType, size }) => ({ name, path, mimeType, size })),
);

function onChange(nextNodes: ComposerNode[]) {
  draftEmpty.value = !nextNodes.some((node) =>
    node.type === "chip" || (node.type === "text" && node.value.replace(/\u00a0/g, " ").trim()),
  );
}

function startReask() {
  if (props.viewOnly || !props.enableReplay) return;
  const serialized = composer.value?.serialize();
  const content = serialized && !serialized.empty
    ? formatAgentPrompt(serialized.nodes, { directMentions: true })
    : props.message.content;
  emit("start-reask", props.message.id, content, replayAttachments.value);
}

function openComposer() {
  if (expanded.value) return;
  expanded.value = true;
  if (props.viewOnly) return;
  void nextTick(() => composer.value?.focusEnd());
}

function onFieldFocus(event: FocusEvent) {
  const target = event.target as HTMLElement | null;
  if (!target?.closest(".composer-field")) return;
  openComposer();
}

function onComposerClick(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  if (target?.closest("button, a, select, input, textarea")) return;
  if (!expanded.value) {
    openComposer();
    return;
  }
  composer.value?.focus();
}

function rejectAttachmentEdit() {
  editNotice.value = "重新提问不能新增附件，只会带上原来的附件";
}

function inlinePreview(attachment: MessageAttachment) {
  return /^(data:|blob:|https?:)/.test(attachment.path) ? attachment.path : "";
}

async function openAttachment(attachment: MessageAttachment) {
  if (inlinePreview(attachment)) return;
  await backend.openMessageAttachment(attachment.id);
}

watch(
  () => props.restoreNonce,
  () => {
    if (!props.restoreNonce || props.restoreMessageId !== props.message.id) return;
    composer.value?.restore(nodes.value);
    draftEmpty.value = false;
    editNotice.value = "";
  },
);

const root = ref<HTMLElement | null>(null);
const seen = ref(false);
let disposed = false;
watch(root, (el, _, onCleanup) => {
  if (el) onCleanup(watchOffscreen(el, (offscreen) => { seen.value ||= !offscreen; }));
}, { flush: "post" });

onBeforeUnmount(() => {
  disposed = true;
  previews.value = {};
});

// 预览是最大约 13MB 的 data URL：滚到可见才取，卸载即释放。
watch(
  () => (seen.value ? pastedImages.value : []),
  async (attachments) => {
    for (const attachment of attachments) {
      if (disposed) return;
      if (previews.value[attachment.id]) continue;
      const inline = inlinePreview(attachment);
      if (inline) {
        previews.value[attachment.id] = inline;
        continue;
      }
      const preview = await backend.loadAttachmentPreview(attachment.id);
      if (preview && !disposed) previews.value[attachment.id] = preview;
    }
  },
  { immediate: true },
);
</script>

<template>
  <article ref="root" class="chat-turn user-turn">
    <ChatComposer
      ref="composer"
      class="user-prompt"
      :class="{ 'is-expanded': expanded, 'is-view-only': viewOnly }"
      @focusin="onFieldFocus"
      placeholder=""
      :initial-nodes="nodes"
      :contexts="contexts"
      :target-label="targetLabel"
      :target-avatar="targetAvatar"
      :target-value="targetValue"
      :target-options="targetOptions"
      :allow-target-change="allowTargetChange && !viewOnly"
      :can-send="!viewOnly && !draftEmpty"
      :disabled="viewOnly"
      :show-context-usage="false"
      footer-mode="focus"
      @change="onChange"
      @submit="startReask"
      @paste-image="rejectAttachmentEdit"
      @drop-workspace-file="rejectAttachmentEdit"
      @drop-os-files="rejectAttachmentEdit"
      @target-change="emit('target-change', $event)"
      @click="onComposerClick"
    >
      <template #actions>
        <template v-if="!viewOnly">
          <UiButton
            v-if="running"
            type="button"
            class="icon-button"
            tip="停止当前任务"
            @click="emit('stop')"
          >
            <Square :size="14" />
          </UiButton>
          <UiButton class="ds-send-button send-button" type="submit" :disabled="draftEmpty" :tip="running ? '加入队列' : '发送'">
            <ArrowUp :size="16" />
          </UiButton>
        </template>
      </template>
      <span v-if="promptTag" class="user-prompt-tag" :data-kind="promptTag.kind">{{ promptTag.label }}</span>
      <p v-if="editNotice" class="composer-error">{{ editNotice }}</p>
      <ComposerPasteImages
        :images="pastedImages.map((image) => ({ key: image.id, src: previews[image.id], name: image.name }))"
      />
      <div v-if="files.length" class="composer-attachments vs-scroll">
        <template v-for="file in files" :key="file.id">
          <UiTooltip :content="file.name">
            <button class="composer-attachment" type="button" @click="openAttachment(file)">
              <FileText :size="15" />
              <span>{{ file.name }}</span>
            </button>
          </UiTooltip>
        </template>
      </div>
    </ChatComposer>
  </article>
</template>
