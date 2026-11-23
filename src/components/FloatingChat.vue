<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { vMotion } from "../ui/motion";
import { t } from "../i18n";
import { Maximize2, MessageCircle, Minimize2, Minus } from "@lucide/vue";
import { useRoute } from "vue-router";
import ChatPanel from "./ChatPanel.vue";
import { dictationService } from "../dictation/dictation-service";
import { useAppStore } from "../stores/app";
import { escapeClosesDialog } from "./ui/dialog-escape";
import { readStorage, writeStorage } from "../utils/safe-storage";

const FLOAT_WIDTH_KEY = "multara.chat.floatWidth";
const FLOAT_MIN_WIDTH = 360;
const FLOAT_DEFAULT_WIDTH = 480;
const FLOAT_EXPANDED_WIDTH = 720;
const FLOAT_MAX_WIDTH = 960;

const props = defineProps<{
  /** 需求工作台等自带聊天时隐藏，但保持挂载以免丢会话 */
  hidden?: boolean;
}>();

const store = useAppStore();
const route = useRoute();
const projectId = computed(() => typeof route.params.projectId === "string" ? route.params.projectId : null);

function clampFloatWidth(width: number) {
  const max = Math.max(FLOAT_MIN_WIDTH, Math.min(FLOAT_MAX_WIDTH, window.innerWidth));
  return Math.round(Math.min(max, Math.max(FLOAT_MIN_WIDTH, width)));
}

function loadFloatWidth() {
  const raw = Number(readStorage(FLOAT_WIDTH_KEY));
  return Number.isFinite(raw) && raw > 0 ? clampFloatWidth(raw) : FLOAT_DEFAULT_WIDTH;
}

const panelWidth = ref(loadFloatWidth());
/** 加宽前的窄宽度，点还原时回到这里 */
const narrowWidth = ref(
  panelWidth.value < FLOAT_EXPANDED_WIDTH ? panelWidth.value : FLOAT_DEFAULT_WIDTH,
);
const expanded = computed(() => panelWidth.value >= FLOAT_EXPANDED_WIDTH);

function persistFloatWidth(width: number) {
  panelWidth.value = clampFloatWidth(width);
  writeStorage(FLOAT_WIDTH_KEY, String(panelWidth.value));
}

function toggleExpand() {
  if (expanded.value) {
    persistFloatWidth(narrowWidth.value);
    return;
  }
  narrowWidth.value = panelWidth.value;
  persistFloatWidth(FLOAT_EXPANDED_WIDTH);
}

function startResize(event: PointerEvent) {
  if (event.button !== 0) return;
  event.preventDefault();
  const handle = event.currentTarget as HTMLElement;
  handle.setPointerCapture(event.pointerId);
  const startX = event.clientX;
  const startWidth = panelWidth.value;
  document.body.classList.add("is-resizing-panels");
  const onMove = (move: PointerEvent) => {
    // 左边缘往左拖 = 加宽
    const next = clampFloatWidth(startWidth + startX - move.clientX);
    panelWidth.value = next;
    if (next < FLOAT_EXPANDED_WIDTH) narrowWidth.value = next;
  };
  const onUp = () => {
    handle.removeEventListener("pointermove", onMove);
    handle.removeEventListener("pointerup", onUp);
    handle.removeEventListener("pointercancel", onUp);
    document.body.classList.remove("is-resizing-panels");
    persistFloatWidth(panelWidth.value);
  };
  handle.addEventListener("pointermove", onMove);
  handle.addEventListener("pointerup", onUp);
  handle.addEventListener("pointercancel", onUp);
}

/** 首次打开后再常驻，避免启动就 ensureSession；收起/切页不再拆掉 */
const retained = ref(false);
watch(
  () => store.chatOpen,
  (open) => {
    if (open) retained.value = true;
    else void dictationService.stop();
  },
  { immediate: true },
);
watch(
  () => props.hidden,
  (hidden) => {
    if (hidden) void dictationService.stop();
  },
);
/** host 已在 hidden 时整体 v-show=false；此处只区分面板 / FAB */
const panelOpen = computed(() => store.chatOpen);
const panelEl = ref<HTMLElement | null>(null);
const chatPanel = ref<{ focus: () => void } | null>(null);
/** 关掉时先播完滑出，再卸掉动画，避免停在带 transform 的状态上 */
const closing = ref(false);
let closeTimer = 0;

watch(panelOpen, (open, wasOpen) => {
  if (open) {
    closing.value = false;
    window.clearTimeout(closeTimer);
    // 隐藏挂载时不抢焦点；首次打开要等 ChatPanel 挂上
    if (!props.hidden) {
      void nextTick(() => chatPanel.value?.focus());
    }
    return;
  }
  if (!wasOpen) return;
  closing.value = true;
  window.clearTimeout(closeTimer);
  closeTimer = window.setTimeout(() => {
    if (!panelOpen.value) closing.value = false;
  }, 250);
});

function onPanelAnimationEnd(event: AnimationEvent) {
  if (event.target !== panelEl.value || event.animationName !== "floating-chat-out") return;
  if (!panelOpen.value) closing.value = false;
}
const fabVisible = computed(() => !store.chatOpen);
/** 收起时有待确认的计划就在气泡按钮上亮点，只看已加载过消息的会话 */
const pendingPlan = computed(() =>
  store.openedSessionIds(projectId.value).some((sessionId) => Boolean(store.planStates(sessionId).latestAwaitingKey)),
);

function close() {
  store.chatOpen = false;
}

function onKeydown(event: KeyboardEvent) {
  // 工作台/收件箱里气泡只是隐藏挂载，不能抢 Esc；有 modal 时留给弹层
  if (props.hidden || !store.chatOpen) return;
  if (!escapeClosesDialog(event)) return;
  if (document.querySelector(".modal-backdrop:not(.ds-motion-ghost)")) return;
  event.preventDefault();
  close();
}

function onWindowResize() {
  persistFloatWidth(panelWidth.value);
}

onMounted(() => {
  document.addEventListener("keydown", onKeydown);
  window.addEventListener("resize", onWindowResize);
});
onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", onWindowResize);
  document.body.classList.remove("is-resizing-panels");
  window.clearTimeout(closeTimer);
});
</script>

<template>
  <!-- 工作台等自带聊天时整块隐藏；ChatPanel 用 retained 常驻，避免拆掉丢会话 -->
  <div v-show="!hidden" class="floating-chat-host">
    <div
      class="floating-chat-backdrop"
      :class="{ open: panelOpen }"
      aria-hidden="true"
      @click="close"
    />

    <section
      ref="panelEl"
      class="floating-chat"
      :class="{ open: panelOpen, closing: closing && !panelOpen }"
      :style="{ width: `${panelWidth}px` }"
      :aria-hidden="!panelOpen"
      @animationend="onPanelAnimationEnd"
    >
      <div
        class="floating-chat-sash"
        role="separator"
        aria-orientation="vertical"
        :aria-valuenow="panelWidth"
        @pointerdown="startResize"
      />
      <ChatPanel
        v-if="retained"
        ref="chatPanel"
        :key="projectId ?? 'global'"
        :project-id="projectId"
        compact
        session-commands
        :show-editing-bar="false"
        :attentive="Boolean(store.chatOpen && !hidden)"
      >
        <template #header-actions>
          <UiButton class="icon-button" variant="ghost" size="icon" :tip="expanded ? t('chat.floatRestore') : t('chat.floatExpand')" @click="toggleExpand">
            <Minimize2 v-if="expanded" :size="15" />
            <Maximize2 v-else :size="15" />
          </UiButton>
          <UiButton class="icon-button" variant="ghost" size="icon" :tip="t('chat.floatMinimize')" @click="close">
            <Minus :size="16" />
          </UiButton>
        </template>
      </ChatPanel>
    </section>

    <UiButton
      v-if="fabVisible"
      class="chat-fab"
      :tip="pendingPlan ? t('chat.plan.pendingBadge') : t('chat.floatOpen')"
      @click="store.chatOpen = true"
    >
      <MessageCircle :size="22" />
      <span v-motion="'scale'" v-if="pendingPlan" class="chat-fab-dot" aria-hidden="true" />
    </UiButton>
  </div>
</template>
