<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { ArrowUp, FileText, X } from "@lucide/vue";
import ChatComposer from "./ChatComposer.vue";
import ChatStopButton from "./ChatStopButton.vue";
import ComposerPasteImages from "./ComposerPasteImages.vue";
import type { SelectOption } from "../ui/select-types";
import type {
  ChatMessage,
  EditorSelectionContext,
  MessageAttachment,
  MessageAttachmentInput,
} from "../../domain/types";
import * as backend from "../../services/backend";
import { useAppStore } from "../../stores/app";
import { genUiReceipt } from "./genui/receipt";
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
const store = useAppStore();
const previews = ref<Record<string, string>>({});
const removedAttachmentIds = ref(new Set<string>());
const editNotice = ref("");
const composer = ref<{
  focus: () => void;
  focusEnd: () => void;
  restore: (nodes: ComposerNode[]) => void;
  serialize: () => { text: string; nodes: ComposerNode[]; empty: boolean };
} | null>(null);
const expanded = ref(false);
const draftEmpty = ref(false);
/** 收起为 3 行时内容是否被裁切（用于底部渐变提示） */
const clipped = ref(false);
/** 折叠态仅一行正文时，停止钮与输入区上下居中；多行仍贴右下角 */
const singleLine = ref(true);
const OVERFLOW_TOLERANCE_PX = 1;
let clipObserver: ResizeObserver | undefined;

const nodes = computed(() => parseUserPromptNodes(props.message.content));
const receipt = computed(() => {
  if (props.message.sourceType !== "genui") return null;
  const messageId = props.message.sourceMeta?.messageId;
  const assistant = typeof messageId === "string"
    ? store.messages[props.message.sessionId]?.find((item) => item.id === messageId)
    : undefined;
  return genUiReceipt(props.message, assistant?.content);
});
const keptAttachments = computed(() =>
  props.message.attachments.filter((item) => !removedAttachmentIds.value.has(item.id)),
);
const pastedImages = computed(() => keptAttachments.value.filter(isPastedImage));
const files = computed(() => keptAttachments.value.filter((item) => !isPastedImage(item)));
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
  keptAttachments.value.map(({ name, path, mimeType, size }) => ({ name, path, mimeType, size })),
);

function removeAttachment(id: string) {
  if (props.viewOnly) return;
  removedAttachmentIds.value = new Set([...removedAttachmentIds.value, id]);
}

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

function measureClip() {
  const field = root.value?.querySelector<HTMLElement>(".composer-field");
  if (!field || expanded.value) {
    clipped.value = false;
    singleLine.value = true;
    return;
  }
  const lineHeight = Number.parseFloat(getComputedStyle(field).lineHeight) || 20;
  // 有附件/图片时整块变高，停止钮继续贴右下，避免居中叠到附件上
  singleLine.value =
    field.scrollHeight <= lineHeight + OVERFLOW_TOLERANCE_PX
    && pastedImages.value.length === 0
    && files.value.length === 0;
  clipped.value = field.scrollHeight > field.clientHeight + OVERFLOW_TOLERANCE_PX;
}

function bindClipObserver() {
  clipObserver?.disconnect();
  clipObserver = undefined;
  const field = root.value?.querySelector<HTMLElement>(".composer-field");
  if (!field || typeof ResizeObserver === "undefined") return;
  clipObserver = new ResizeObserver(() => measureClip());
  clipObserver.observe(field);
}

function openComposer() {
  if (expanded.value) return;
  expanded.value = true;
  clipped.value = false;
  // 只读也可聚焦以便选中复制
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
  () => props.message.id,
  () => {
    removedAttachmentIds.value = new Set();
  },
);

watch(
  () => props.restoreNonce,
  () => {
    if (!props.restoreNonce || props.restoreMessageId !== props.message.id) return;
    composer.value?.restore(nodes.value);
    removedAttachmentIds.value = new Set();
    draftEmpty.value = false;
    editNotice.value = "";
  },
);

const root = ref<HTMLElement | null>(null);
const seen = ref(false);
let disposed = false;
watch(root, (el, _, onCleanup) => {
  if (!el) return;
  onCleanup(watchOffscreen(el, (offscreen) => { seen.value ||= !offscreen; }));
  void nextTick(() => {
    bindClipObserver();
    measureClip();
  });
}, { flush: "post" });

watch([expanded, () => props.message.content, pastedImages, files], () => {
  void nextTick(measureClip);
});

onBeforeUnmount(() => {
  disposed = true;
  clipObserver?.disconnect();
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
    <slot name="source" />
    <div v-if="receipt" class="genui-receipt">
      <header class="genui-receipt-head">
        <strong v-if="receipt.title">{{ receipt.title }}</strong>
        <span v-if="receipt.actionLabel">{{ receipt.actionLabel }}</span>
      </header>
      <div v-for="row in receipt.rows" :key="row.label" class="genui-receipt-row">
        <span class="genui-receipt-label">{{ row.label }}</span>
        <span class="genui-receipt-values">
          <span v-for="value in row.values" :key="value" class="genui-receipt-chip">{{ value }}</span>
        </span>
      </div>
    </div>
    <ChatComposer
      v-else
      ref="composer"
      class="user-prompt"
      :class="{
        'is-expanded': expanded,
        'is-view-only': viewOnly,
        'has-overflow': clipped,
        'is-running': running && !viewOnly,
      }"
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
      :readonly="viewOnly"
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
      <!-- 折叠未聚焦：停止钮在右侧；单行与输入上下居中，多行贴右下；点开后由 footer 接手 -->
      <div
        v-if="running && !viewOnly"
        class="user-prompt-stop-wrap"
        :class="{ 'is-single-line': singleLine }"
      >
        <ChatStopButton @click="emit('stop')" />
      </div>
      <template #actions>
        <template v-if="!viewOnly">
          <ChatStopButton v-if="running" @click="emit('stop')" />
          <UiButton
            v-else
            class="ds-send-button send-button"
            type="submit"
            :disabled="draftEmpty"
            :tip="t('chat.send')"
          >
            <ArrowUp :size="16" />
          </UiButton>
        </template>
      </template>
      <span v-if="promptTag" class="user-prompt-tag" :data-kind="promptTag.kind">{{ promptTag.label }}</span>
      <p v-if="editNotice" class="composer-error">{{ editNotice }}</p>
      <ComposerPasteImages
        :removable="!viewOnly"
        :images="pastedImages.map((image) => ({ key: image.id, src: previews[image.id], name: image.name }))"
        @remove="removeAttachment"
      />
      <div v-if="files.length" class="composer-attachments vs-scroll">
        <template v-for="file in files" :key="file.id">
          <UiTooltip :content="file.name">
            <div class="composer-attachment" role="button" tabindex="0" @click="openAttachment(file)">
              <FileText :size="15" />
              <span>{{ file.name }}</span>
              <UiButton
                v-if="!viewOnly"
                type="button"
                variant="ghost"
                size="icon"
                :tip="t('chat.removeAttachment')"
                @click.stop="removeAttachment(file.id)"
              >
                <X :size="12" />
              </UiButton>
            </div>
          </UiTooltip>
        </template>
      </div>
    </ChatComposer>
  </article>
</template>
