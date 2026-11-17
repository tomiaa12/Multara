<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ArrowLeft, ArrowRight, ExternalLink, RotateCw } from "@lucide/vue";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import { inTauri } from "../../services/dev-backend";
import Codicon from "../../components/ui/Codicon.vue";
import UiButton from "../../components/ui/UiButton.vue";
import { normalizeBrowserUrl } from "./inbox-browser-links";

const props = withDefaults(
  defineProps<{
    links: string[];
    openUrl?: string;
    /** 当前 surface 是否可见；切走时隐藏子 Webview */
    active?: boolean;
  }>(),
  { active: true },
);

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

let resizeObserver: ResizeObserver | undefined;
let unlistenNavigated: UnlistenFn | undefined;
let syncTimer: ReturnType<typeof setTimeout> | undefined;
let lastSyncedUrl = "";

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
  const visible =
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

onBeforeUnmount(() => {
  if (syncTimer) clearTimeout(syncTimer);
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
      />
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
