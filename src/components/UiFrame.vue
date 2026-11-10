<script setup lang="ts">
// Canvas 页面的外壳（方案 §5.7、§8）：工具栏 + 状态提示 + 一个占位（slot）。
// 真正的 iframe 在全局的 CanvasFrameHost 里，盖在这个占位上：keep-alive 把页面藏起来时
// iframe 不会被卸载，切走后智能体的 canvas action 照样能执行。
// - 同一实例同时只在一个视图里显示（R16）：别处在前台显示时这里只给占位和「在此显示」。
// - 状态机：loading → ready；服务断开 → reconnecting（遮罩盖住最后一帧，每秒探测）→ 30 秒后 unavailable（R17）。
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from "vue";
import { canvasKey, useCanvasStore } from "../stores/canvas";
import { openExternalUrl } from "../services/backend";
import { t } from "../i18n";

const props = defineProps<{
  sessionId: string;
  instanceId: string;
  title: string;
  /** 显示位置的唯一 id（工作台标签、聊天弹窗……），用于单视图认领 */
  viewId: string;
  /** 在弹窗里：工具栏多一个 ×，只关窗口（Canvas 还在）；结束实例是单独的垃圾桶按钮 */
  dismissible?: boolean;
}>();
const emit = defineEmits<{
  /** 结束这个 Canvas 实例（从列表里删掉） */
  close: [];
  /** 只关掉显示它的窗口 */
  dismiss: [];
}>();

const RECONNECT_LIMIT_MS = 30_000;
const PROBE_MS = 5_000;

const canvas = useCanvasStore();
type Phase = "loading" | "ready" | "reconnecting" | "unavailable" | "elsewhere";
const phase = ref<Phase>("loading");
const url = ref("");
const frameKey = ref(0);
const error = ref("");
const slot = ref<HTMLElement | null>(null);
let probeTimer = 0;
let reconnectTimer = 0;
let reconnectStarted = 0;
let disposed = false;

const key = computed(() => canvasKey(props.sessionId, props.instanceId));
const owner = computed(() => canvas.claimedBy(props.sessionId, props.instanceId));
const mine = computed(() => owner.value === props.viewId);
/** 别的视图正在前台显示它；后台的认领不算（那边只是保持连接） */
const shownElsewhere = computed(() => Boolean(owner.value && !mine.value && canvas.isViewActive(owner.value)));
const loaded = computed(() => {
  const entry = canvas.live[key.value];
  return Boolean(entry && entry.loadedFrameKey === entry.frameKey && entry.url === url.value);
});
const pinned = computed(() =>
  Object.values(canvas.instances)
    .flat()
    .some((item) => item.sessionId === props.sessionId && item.instanceId === props.instanceId && item.pinned),
);

function currentTheme() {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

/** 把当前地址交给全局 iframe 层 */
function publish() {
  if (!url.value || !mine.value) return;
  canvas.setLive(props.sessionId, props.instanceId, {
    url: url.value,
    title: props.title,
    frameKey: frameKey.value,
    theme: currentTheme(),
  });
}

function togglePin() {
  void canvas.setPinned(props.sessionId, props.instanceId, !pinned.value);
}

async function host(force = false) {
  error.value = "";
  try {
    const next = await canvas.ensureUrl(props.sessionId, props.instanceId, force);
    if (disposed) return;
    const existing = canvas.live[key.value];
    if (existing && existing.url === next && !force) {
      // 后台一直连着的页面：直接接回来，不重新加载
      frameKey.value = existing.frameKey;
      url.value = next;
    } else if (next !== url.value) {
      url.value = next;
      frameKey.value += 1;
    }
    publish();
    phase.value = loaded.value ? "ready" : "loading";
    schedulePoll();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    startReconnect();
  }
}

function schedulePoll() {
  window.clearInterval(probeTimer);
  probeTimer = window.setInterval(async () => {
    if (!mine.value || phase.value === "reconnecting" || phase.value === "unavailable") return;
    const alive = await canvas.probe(props.sessionId, props.instanceId);
    if (!alive && !disposed) startReconnect();
  }, PROBE_MS);
}

function startReconnect() {
  if (disposed) return;
  if (phase.value !== "reconnecting") reconnectStarted = Date.now();
  phase.value = "reconnecting";
  window.clearTimeout(reconnectTimer);
  reconnectTimer = window.setTimeout(async () => {
    if (disposed || !mine.value) return;
    if (Date.now() - reconnectStarted > RECONNECT_LIMIT_MS) {
      phase.value = "unavailable";
      return;
    }
    try {
      const next = await canvas.ensureUrl(props.sessionId, props.instanceId, true);
      if (disposed) return;
      url.value = next;
      frameKey.value += 1;
      error.value = "";
      publish();
      phase.value = "loading";
      schedulePoll();
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
      startReconnect();
    }
  }, 1_000);
}

function takeOver() {
  canvas.claim(props.sessionId, props.instanceId, props.viewId);
}

function retry() {
  reconnectStarted = Date.now();
  phase.value = "loading";
  void host(true);
}

function reload() {
  frameKey.value += 1;
  phase.value = "loading";
  publish();
}

function openInBrowser() {
  if (url.value) void openExternalUrl(url.value);
}

watch(loaded, (value) => {
  if (value && phase.value === "loading") phase.value = "ready";
});

watch(mine, (value) => {
  if (value) {
    void host();
  } else {
    window.clearInterval(probeTimer);
    window.clearTimeout(reconnectTimer);
  }
});

watch(
  () => [props.sessionId, props.instanceId],
  () => {
    url.value = "";
    if (!owner.value) takeOver();
    else if (mine.value) void host();
  },
);

watch(
  () => props.title,
  () => publish(),
);

// iframe 层看这个决定藏不藏：重连 / 不可用 / 在别处显示时让外壳的提示露出来
const veil = computed(() => phase.value === "reconnecting" || phase.value === "unavailable" || !mine.value);
watch(veil, (value) => canvas.setVeiled(key.value, value), { immediate: true });

// 别的视图放手了：没人显示就由这里接管
watch(owner, (value) => {
  if (!value && !disposed && canvas.isViewActive(props.viewId)) takeOver();
});
// 别处的认领只是后台保持连接、而这里在前台：接过来显示
watch(
  () => [owner.value, owner.value ? canvas.isViewActive(owner.value) : false] as const,
  ([value, ownerActive]) => {
    if (value && value !== props.viewId && !ownerActive && canvas.isViewActive(props.viewId)) takeOver();
  },
);

// keep-alive：藏起来时只标记为后台（认领保留，iframe 在全局层继续连着），回到前台再显示
onDeactivated(() => {
  canvas.setViewActive(props.viewId, false);
  canvas.pruneLive();
});
onActivated(() => {
  canvas.setViewActive(props.viewId, true);
  if (!owner.value || !canvas.isViewActive(owner.value)) takeOver();
  if (mine.value) void host();
});

onMounted(() => {
  canvas.setViewActive(props.viewId, true);
  canvas.registerSlot(props.viewId, slot.value);
  // 没人在前台显示它就认领；别处在前台显示时只给占位，用户点「在此显示」才转过来
  if (!owner.value || !canvas.isViewActive(owner.value)) takeOver();
  if (mine.value) void host();
});

onBeforeUnmount(() => {
  disposed = true;
  window.clearInterval(probeTimer);
  window.clearTimeout(reconnectTimer);
  canvas.registerSlot(props.viewId, null);
  canvas.setViewActive(props.viewId, null);
  canvas.setVeiled(key.value, false);
  // 真正卸载（关标签、切会话换出）：放手；页面在全局层里按后台预算保留一段时间
  canvas.release(props.sessionId, props.instanceId, props.viewId);
  canvas.pruneLive();
});
</script>

<template>
  <div class="ui-frame" :data-phase="shownElsewhere ? 'elsewhere' : phase">
    <div class="ui-frame-toolbar">
      <span class="ui-frame-title">{{ title }}</span>
      <UiButton
        size="icon"
        variant="ghost"
        :tip="pinned ? t('canvas.unpin') : t('canvas.pin')"
        :aria-pressed="pinned"
        @click="togglePin"
      >
        <Codicon :name="pinned ? 'pinned' : 'pin'" :size="14" />
      </UiButton>
      <UiButton size="icon" variant="ghost" :tip="t('canvas.reload')" :disabled="!mine || !url" @click="reload">
        <Codicon name="refresh" :size="14" />
      </UiButton>
      <UiButton size="icon" variant="ghost" :tip="t('canvas.openInBrowser')" :disabled="!url" @click="openInBrowser">
        <Codicon name="link-external" :size="14" />
      </UiButton>
      <UiButton
        size="icon"
        variant="ghost"
        :tip="t('canvas.endInstance')"
        :aria-label="t('canvas.endInstance')"
        @click="emit('close')"
      >
        <Codicon name="trash" :size="14" />
      </UiButton>
      <UiButton
        v-if="dismissible"
        size="icon"
        variant="ghost"
        :tip="t('common.close')"
        :aria-label="t('common.close')"
        @click="emit('dismiss')"
      >
        <Codicon name="close" :size="14" />
      </UiButton>
    </div>
    <div class="ui-frame-body">
      <!-- 全局 iframe 层盖在这个占位上 -->
      <div ref="slot" class="ui-frame-slot" />
      <div v-if="shownElsewhere" class="ui-frame-placeholder">
        <p>{{ t("canvas.shownElsewhere") }}</p>
        <UiButton size="small" @click="takeOver">{{ t("canvas.showHere") }}</UiButton>
      </div>
      <template v-else-if="mine">
        <div v-if="phase === 'loading' && !loaded" class="ui-frame-overlay">
          <span>{{ t("canvas.loading") }}</span>
        </div>
        <div v-else-if="phase === 'reconnecting'" class="ui-frame-overlay frozen" role="status">
          <span>{{ t("canvas.reconnecting") }}</span>
        </div>
        <div v-else-if="phase === 'unavailable'" class="ui-frame-overlay" role="alert">
          <p>{{ t("canvas.unavailable") }}</p>
          <p v-if="error" class="ui-frame-error">{{ error }}</p>
          <UiButton size="small" @click="retry">{{ t("canvas.retry") }}</UiButton>
        </div>
      </template>
    </div>
  </div>
</template>
