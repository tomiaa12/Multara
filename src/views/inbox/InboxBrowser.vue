<script setup lang="ts">
import { computed, nextTick, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ArrowLeft, ArrowRight, ChevronDown, ExternalLink, RotateCw } from "@lucide/vue";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import { inTauri } from "../../services/dev-backend";
import { useCanvasStore } from "../../stores/canvas";
import { fillComposerFromUi } from "../../components/chat/ui-outbound";
import Codicon from "../../components/ui/Codicon.vue";
import UiButton from "../../components/ui/UiButton.vue";
import { attachBrowserImage } from "./browser-chat-attach";
import { normalizeBrowserUrl } from "./inbox-browser-links";
import {
  AREA_PICK_SCRIPT,
  STOP_PICK_SCRIPT,
  chatMenuScript,
  elementPickScript,
} from "./inbox-browser-scripts";

const props = withDefaults(
  defineProps<{
    links: string[];
    sessionId: string;
    openUrl?: string;
    /** 当前 surface 是否可见；切走时隐藏子 Webview */
    active?: boolean;
  }>(),
  { active: true },
);

const emit = defineEmits<{
  navigate: [url: string];
}>();

const canvas = useCanvasStore();

const useNative = inTauri();

const stack = ref<string[]>([]);
const index = ref(-1);
const draft = ref("");
const reloadNonce = ref(0);
const picker = ref<"" | "history" | "address">("");
const pickerQuery = ref("");
const pickerIndex = ref(0);
const pickerInput = ref<HTMLInputElement | null>(null);
const frameHost = ref<HTMLElement | null>(null);
const chatMenuOpen = ref(false);
const chatBusy = ref(false);
const pickMode = ref<"" | "select" | "comment" | "area">("");

const pageLive = ref(true);
let resizeObserver: ResizeObserver | undefined;
let unlistenNavigated: UnlistenFn | undefined;
let syncTimer: ReturnType<typeof setTimeout> | undefined;
let lastSyncedUrl = "";
let pickPollTimer: ReturnType<typeof setInterval> | undefined;
let chatMenuPollTimer: ReturnType<typeof setInterval> | undefined;

const current = computed(() => stack.value[index.value] ?? "");
const canBack = computed(() => (useNative ? !!current.value : index.value > 0));
const canForward = computed(() =>
  useNative
    ? !!current.value
    : index.value >= 0 && index.value < stack.value.length - 1,
);
const showFrame = computed(() => !!current.value);
const historyMatches = computed(() => {
  const query = pickerQuery.value.trim().toLowerCase();
  if (!query) return props.links;
  return props.links.filter((url) => url.toLowerCase().includes(query));
});
const pickerPlaceholder = computed(() => {
  if (picker.value === "history") return t("inbox.tool.historyPlaceholder");
  return t("inbox.tool.address");
});

function navigate(raw: string) {
  const url = normalizeBrowserUrl(raw);
  if (!url) return;
  stack.value = [...stack.value.slice(0, index.value + 1), url];
  index.value = stack.value.length - 1;
  draft.value = url;
  void nextTick(() => scheduleSync(true));
}

function back() {
  if (!canBack.value) return;
  if (useNative) {
    void backend.inboxBrowserBack();
    return;
  }
  index.value -= 1;
  draft.value = current.value;
}

function forward() {
  if (!canForward.value) return;
  if (useNative) {
    void backend.inboxBrowserForward();
    return;
  }
  index.value += 1;
  draft.value = current.value;
}

function reload() {
  if (!current.value) return;
  if (useNative) {
    void backend.inboxBrowserReload();
    return;
  }
  reloadNonce.value += 1;
}

function onSubmit() {
  navigate(draft.value);
}

/** eval 结果有时是 JSON 字符串再套一层。菜单和选区都要解到对象。 */
function evalJson<T>(raw: string): T | null {
  let value: unknown = raw;
  for (let depth = 0; depth < 2; depth += 1) {
    if (typeof value !== "string") break;
    const text = value.trim();
    if (!text || text === "null") return null;
    try {
      value = JSON.parse(text);
    } catch {
      return null;
    }
  }
  if (!value || typeof value !== "object") return null;
  return value as T;
}

/** 地址栏在主 WebView 里。子页面抢到第一响应者后，中文输入法会打出方框。对齐 VS Code：编辑地址时把焦点还给主页面。 */
function focusHost() {
  if (!useNative) return;
  void getCurrentWebview().setFocus();
}

function openPicker(kind: "history" | "address") {
  picker.value = kind;
  pickerQuery.value = "";
  pickerIndex.value = 0;
  scheduleSync(false);
  void nextTick(() => pickerInput.value?.focus());
}

function closePicker() {
  picker.value = "";
  scheduleSync(false);
}

function movePicker(delta: number) {
  const count = picker.value === "history" ? historyMatches.value.length : 0;
  if (!count) return;
  pickerIndex.value = (pickerIndex.value + delta + count) % count;
}

function confirmPicker() {
  if (picker.value === "history") {
    const url = historyMatches.value[pickerIndex.value];
    if (url) navigate(url);
    closePicker();
    return;
  }
  navigate(pickerQuery.value);
  closePicker();
}

function openUrlFromProp(raw: string) {
  const line = raw.includes("\n") ? raw.slice(raw.indexOf("\n") + 1) : raw;
  if (line) navigate(line);
}

function readBounds() {
  const el = frameHost.value;
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  if (rect.width < 2 || rect.height < 2) return null;
  return {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

function scheduleSync(forceUrl: boolean) {
  if (!useNative) return;
  if (syncTimer) clearTimeout(syncTimer);
  syncTimer = setTimeout(() => {
    syncTimer = undefined;
    void syncNative(forceUrl);
  }, 16);
}

async function syncNative(forceUrl: boolean) {
  if (!useNative) return;
  const url = current.value;
  // 选元素 / 区域截图 / 页内悬浮菜单时必须保持 webview 可见
  const visible =
    pageLive.value &&
    !!props.active &&
    !!url &&
    !picker.value &&
    document.visibilityState !== "hidden";
  const bounds = readBounds();
  if (!visible || !bounds) {
    await backend.inboxBrowserSet({
      url: null,
      x: -10_000,
      y: -10_000,
      width: 1,
      height: 1,
      visible: false,
    });
    return;
  }
  const nextUrl = forceUrl || url !== lastSyncedUrl ? url : null;
  await backend.inboxBrowserSet({
    url: nextUrl,
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
    visible: true,
  });
  if (nextUrl) lastSyncedUrl = nextUrl;
}

function onNativeNavigated(url: string) {
  if (!url || !/^https?:\/\//i.test(url)) return;
  draft.value = url;
  if (stack.value[index.value] !== url) {
    // 页内跳转：对齐地址栏，并写入栈以便 iframe 回退路径一致
    stack.value = [...stack.value.slice(0, index.value + 1), url];
    index.value = stack.value.length - 1;
  }
  lastSyncedUrl = url;
  emit("navigate", url);
  const active = document.activeElement;
  if (active instanceof HTMLElement && active.classList.contains("inbox-browser-address")) {
    focusHost();
  }
}

watch(historyMatches, () => {
  pickerIndex.value = 0;
});

watch(
  () => props.openUrl,
  (url) => {
    if (url) openUrlFromProp(url);
  },
);

watch(
  () => [props.active, current.value, picker.value] as const,
  () => scheduleSync(false),
);

onActivated(() => {
  pageLive.value = true;
  scheduleSync(false);
});

onDeactivated(() => {
  pageLive.value = false;
  void closeChatMenu();
  void stopPagePick();
  scheduleSync(false);
});

onMounted(async () => {
  if (!useNative) return;
  if (typeof ResizeObserver !== "undefined") {
    resizeObserver = new ResizeObserver(() => scheduleSync(false));
    if (frameHost.value) resizeObserver.observe(frameHost.value);
  }
  window.addEventListener("resize", onWindowResize);
  document.addEventListener("visibilitychange", onVisibility);
  unlistenNavigated = await listen<{ url: string }>("inbox-browser:navigated", (event) => {
    onNativeNavigated(event.payload?.url ?? "");
  });
  scheduleSync(true);
});

function onWindowResize() {
  scheduleSync(false);
}

function onVisibility() {
  scheduleSync(false);
}

watch(frameHost, (el, prev) => {
  if (!useNative || !resizeObserver) return;
  if (prev) resizeObserver.unobserve(prev);
  if (el) resizeObserver.observe(el);
  scheduleSync(false);
});

function stopPickPolling() {
  if (pickPollTimer) {
    clearInterval(pickPollTimer);
    pickPollTimer = undefined;
  }
}

function stopChatMenuPolling() {
  if (chatMenuPollTimer) {
    clearInterval(chatMenuPollTimer);
    chatMenuPollTimer = undefined;
  }
}

async function closeChatMenu() {
  stopChatMenuPolling();
  chatMenuOpen.value = false;
  if (!useNative) return;
  try {
    await backend.inboxBrowserEval(`(() => {
      const menu = document.getElementById('__multara-chat-menu');
      if (menu) menu.remove();
      window.__multaraChatMenuActive = false;
      window.__multaraChatMenu = null;
      true;
    })()`);
  } catch {
    // 页面可能已卸载
  }
}

async function stopPagePick() {
  stopPickPolling();
  pickMode.value = "";
  if (useNative) {
    try {
      await backend.inboxBrowserEval(STOP_PICK_SCRIPT);
    } catch {
      // 页面可能已卸载
    }
  }
}

async function openChatMenu() {
  if (!useNative || !current.value || chatBusy.value) return;
  if (chatMenuOpen.value) {
    await closeChatMenu();
    return;
  }
  await stopPagePick();
  chatMenuOpen.value = true;
  await syncNative(false);
  await backend.inboxBrowserEval(
    chatMenuScript([
      {
        id: "element",
        label: t("inbox.browser.addElement"),
        key: "⇧⌘C",
        checked: pickMode.value === "select",
      },
      {
        id: "comment",
        label: t("inbox.browser.commentElements"),
        key: "⌥⌘C",
        checked: pickMode.value === "comment",
      },
      { id: "sep-1", label: "", sep: true },
      { id: "console", label: t("inbox.browser.addConsole") },
      { id: "sep-2", label: "", sep: true },
      { id: "screenshot", label: t("inbox.browser.addScreenshot") },
      {
        id: "area",
        label: t("inbox.browser.addAreaScreenshot"),
        checked: pickMode.value === "area",
      },
    ]),
  );
  stopChatMenuPolling();
  chatMenuPollTimer = setInterval(() => {
    void (async () => {
      try {
        const raw = await backend.inboxBrowserEval(
          "window.__multaraChatMenu ? JSON.stringify(window.__multaraChatMenu) : 'null'",
        );
        const payload = evalJson<{ id?: string }>(raw);
        if (!payload) return;
        await backend.inboxBrowserEval("window.__multaraChatMenu = null");
        stopChatMenuPolling();
        chatMenuOpen.value = false;
        const id = payload.id?.trim();
        if (!id) return;
        await onChatMenuSelect(id);
      } catch {
        // 忽略单次轮询失败
      }
    })();
  }, 120);
}

function attachBrowserContext(label: string, sourceId: string, text: string, data?: unknown) {
  canvas.attachContext({
    sessionId: props.sessionId,
    source: "browser",
    sourceId,
    label,
    text,
    data: data ?? null,
  });
}

async function attachScreenshot(rect?: { x: number; y: number; width: number; height: number }) {
  const base64 = await backend.inboxBrowserScreenshot(rect);
  attachBrowserImage({
    sessionId: props.sessionId,
    name: rect ? "browser-area.png" : "browser.png",
    mimeType: "image/png",
    base64,
  });
}

async function runElementPick(mode: "select" | "comment") {
  if (pickMode.value === mode) {
    await stopPagePick();
    return;
  }
  await stopPagePick();
  pickMode.value = mode;
  await backend.inboxBrowserEval(elementPickScript(mode));
  stopPickPolling();
  pickPollTimer = setInterval(() => {
    void (async () => {
      try {
        const raw = await backend.inboxBrowserEval(
          "window.__multaraPick ? JSON.stringify(window.__multaraPick) : 'null'",
        );
        const payload = evalJson<{
          cancelled?: boolean;
          mode?: string;
          tag?: string;
          selector?: string;
          text?: string;
          html?: string;
        }>(raw);
        if (!payload) return;
        await backend.inboxBrowserEval("window.__multaraPick = null");
        stopPickPolling();
        pickMode.value = "";
        if (payload.cancelled) return;
        const label = payload.selector || payload.tag || t("inbox.browser.element");
        const parts = [
          payload.selector ? `selector: ${payload.selector}` : "",
          payload.text ? `text:\n${payload.text}` : "",
          payload.html ? `html:\n\`\`\`html\n${payload.html}\n\`\`\`` : "",
        ].filter(Boolean);
        attachBrowserContext(label, `element-${Date.now()}`, parts.join("\n\n"), payload);
        if (mode === "comment") {
          fillComposerFromUi({
            sessionId: props.sessionId,
            source: "browser",
            sourceId: "element-comment",
            text: t("inbox.browser.commentPrompt"),
          });
        }
      } catch {
        // 忽略单次轮询失败
      }
    })();
  }, 200);
}

async function runAreaPick() {
  if (pickMode.value === "area") {
    await stopPagePick();
    return;
  }
  await stopPagePick();
  pickMode.value = "area";
  await backend.inboxBrowserEval(AREA_PICK_SCRIPT);
  stopPickPolling();
  pickPollTimer = setInterval(() => {
    void (async () => {
      try {
        const raw = await backend.inboxBrowserEval(
          "window.__multaraArea ? JSON.stringify(window.__multaraArea) : 'null'",
        );
        const payload = evalJson<{
          cancelled?: boolean;
          x?: number;
          y?: number;
          width?: number;
          height?: number;
        }>(raw);
        if (!payload) return;
        await backend.inboxBrowserEval("window.__multaraArea = null");
        stopPickPolling();
        pickMode.value = "";
        if (payload.cancelled || payload.width == null || payload.height == null) return;
        // 先去掉页内遮罩再截，避免虚线框进图
        await new Promise((resolve) => setTimeout(resolve, 40));
        await attachScreenshot({
          x: payload.x ?? 0,
          y: payload.y ?? 0,
          width: payload.width,
          height: payload.height,
        });
      } catch {
        // 忽略单次轮询失败
      }
    })();
  }, 200);
}

async function onChatMenuSelect(id: string) {
  if (!useNative || !current.value || chatBusy.value) return;
  chatBusy.value = true;
  try {
    await syncNative(false);
    if (id === "element") {
      await runElementPick("select");
      return;
    }
    if (id === "comment") {
      await runElementPick("comment");
      return;
    }
    if (id === "console") {
      const logs = await backend.inboxBrowserConsoleLogs();
      if (!logs.trim()) {
        attachBrowserContext(
          t("inbox.browser.consoleLogs"),
          "console",
          t("inbox.browser.consoleEmpty"),
        );
        return;
      }
      attachBrowserContext(t("inbox.browser.consoleLogs"), "console", logs);
      return;
    }
    if (id === "screenshot") {
      await stopPagePick();
      await attachScreenshot();
      return;
    }
    if (id === "area") {
      await runAreaPick();
    }
  } catch (error) {
    console.error(error);
  } finally {
    chatBusy.value = false;
  }
}

watch(
  () => props.active,
  (active) => {
    if (!active) {
      void closeChatMenu();
      void stopPagePick();
    }
  },
);

onBeforeUnmount(() => {
  if (syncTimer) clearTimeout(syncTimer);
  stopPickPolling();
  stopChatMenuPolling();
  window.removeEventListener("resize", onWindowResize);
  document.removeEventListener("visibilitychange", onVisibility);
  resizeObserver?.disconnect();
  void unlistenNavigated?.();
  if (useNative) void backend.inboxBrowserDestroy();
});
</script>

<template>
  <section class="inbox-browser">
    <form class="inbox-browser-bar" @submit.prevent="onSubmit">
      <UiButton variant="ghost" size="icon" :tip="t('inbox.tool.back')" :disabled="!canBack" @click="back">
        <ArrowLeft :size="14" />
      </UiButton>
      <UiButton variant="ghost" size="icon" :tip="t('inbox.tool.forward')" :disabled="!canForward" @click="forward">
        <ArrowRight :size="14" />
      </UiButton>
      <UiButton variant="ghost" size="icon" :tip="t('inbox.tool.reload')" :disabled="!current" @click="reload">
        <RotateCw :size="14" />
      </UiButton>
      <input
        v-model="draft"
        class="inbox-browser-address"
        type="text"
        spellcheck="false"
        :placeholder="t('inbox.tool.address')"
        :aria-label="t('inbox.tool.address')"
        @pointerdown="focusHost"
        @focus="focusHost"
      />
      <UiButton
        v-if="useNative"
        variant="ghost"
        size="small"
        class="inbox-browser-chat-btn"
        :tip="t('inbox.browser.addToChat')"
        :disabled="!current || chatBusy"
        :aria-pressed="Boolean(pickMode) || chatMenuOpen"
        @click="openChatMenu"
      >
        <Codicon name="inspect" :size="14" />
        <ChevronDown :size="12" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :tip="t('inbox.tool.openExternal')"
        :disabled="!current"
        @click="backend.openExternalUrl(current)"
      >
        <ExternalLink :size="14" />
      </UiButton>
    </form>
    <div
      v-show="showFrame"
      ref="frameHost"
      class="inbox-browser-frame"
      :data-native="useNative ? '1' : undefined"
    >
      <iframe
        v-if="!useNative && current"
        :key="`${current}\n${reloadNonce}`"
        class="inbox-browser-iframe"
        :src="current"
        :title="current"
      />
    </div>
    <div v-if="!showFrame" class="inbox-browser-empty">
      <div class="inbox-browser-actions sessions-watermark-actions" role="toolbar" :aria-label="t('inbox.tool.browser')">
        <button type="button" class="sessions-add-tab-watermark-entry" @click="openPicker('history')">
          <Codicon name="history" class="sessions-add-tab-watermark-icon" :size="16" />
          <span class="sessions-add-tab-watermark-label">{{ t("inbox.tool.history") }}</span>
        </button>
        <button type="button" class="sessions-add-tab-watermark-entry" @click="openPicker('address')">
          <Codicon name="link" class="sessions-add-tab-watermark-icon" :size="16" />
          <span class="sessions-add-tab-watermark-label">{{ t("inbox.tool.openAddress") }}</span>
        </button>
      </div>
    </div>
    <div
      v-if="picker"
      class="quick-pick"
      role="dialog"
      :aria-label="pickerPlaceholder"
    >
      <input
        ref="pickerInput"
        v-model="pickerQuery"
        :placeholder="pickerPlaceholder"
        :aria-label="pickerPlaceholder"
        @keydown.escape.stop="closePicker"
        @keydown.enter.prevent="confirmPicker"
        @keydown.down.prevent="movePicker(1)"
        @keydown.up.prevent="movePicker(-1)"
      />
      <template v-if="picker === 'history'">
        <button
          v-for="(url, itemIndex) in historyMatches"
          :key="url"
          type="button"
          :class="{ active: itemIndex === pickerIndex }"
          @click="navigate(url); closePicker()"
          @mouseenter="pickerIndex = itemIndex"
        >
          <Codicon name="globe" :size="16" />
          <span>{{ url }}</span>
        </button>
        <p v-if="!historyMatches.length">{{ t("inbox.tool.noPages") }}</p>
      </template>
      <button
        v-else-if="pickerQuery.trim()"
        type="button"
        class="active"
        @click="confirmPicker"
      >
        <Codicon name="arrow-right" :size="16" />
        <span>{{ t("inbox.tool.goTo", { url: pickerQuery.trim() }) }}</span>
      </button>
    </div>
  </section>
</template>
