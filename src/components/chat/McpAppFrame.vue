<script setup lang="ts">
// 工具结果下方的 MCP App 界面（方案 §7）：iframe + postMessage 桥。
// - 只有这里挂桥；Canvas（UiFrame）永远没有桥。
// - 校验 event.source 是本 iframe、event.origin 是该 server 的托管源，否则丢弃。
// - 离开视口超过 30 秒卸载 iframe，留同高度占位；回来重新加载并重走初始化（R23）。
// - 高度按工具调用缓存，重建时不跳。
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { TimelineItem } from "../../domain/types";
import * as backend from "../../services/backend";
import type { McpAppInfo } from "../../services/backend";
import { useCanvasStore } from "../../stores/canvas";
import { fillComposerFromUi } from "./ui-outbound";
import { McpAppBridge, hostStyleVariables } from "./McpAppBridge";
import { locale } from "../../i18n";

const props = defineProps<{
  sessionId: string;
  toolUse: TimelineItem;
  toolResult?: TimelineItem | null;
}>();

const OFFSCREEN_UNLOAD_MS = 30_000;
const DEFAULT_HEIGHT = 160;
const heightCache = (globalThis as { __mcpAppHeights?: Map<string, number> }).__mcpAppHeights
  ??= new Map<string, number>();

const canvas = useCanvasStore();
const root = ref<HTMLElement | null>(null);
const frame = ref<HTMLIFrameElement | null>(null);
const info = ref<McpAppInfo | null>(null);
const resolved = ref(false);
const failed = ref("");
const visible = ref(false);
const loaded = ref(false);
const callKey = computed(() => props.toolUse.callId ?? `${props.toolUse.seq}`);
const height = ref(heightCache.get(callKey.value) ?? DEFAULT_HEIGHT);
let bridge: McpAppBridge | null = null;
let observer: IntersectionObserver | null = null;
let themeObserver: MutationObserver | null = null;
let offscreenTimer = 0;
let version = "";

function toolInput() {
  const input = props.toolUse.input;
  if (input && typeof input === "object" && "__unparsedToolInput" in input) return (input as Record<string, unknown>).__unparsedToolInput;
  return input ?? {};
}

/** 原始 CallToolResult；没有就把文本包成 content */
function toolResult() {
  const result = props.toolResult;
  if (!result) return undefined;
  if (result.outputJson) return result.outputJson;
  return {
    content: [{ type: "text", text: result.output ?? "" }],
    ...(result.status === "error" ? { isError: true } : {}),
  };
}

function hostContext() {
  const styles = getComputedStyle(document.documentElement);
  return {
    theme: document.documentElement.dataset.theme === "light" ? "light" : "dark",
    styles: { variables: hostStyleVariables((name) => styles.getPropertyValue(name)) },
    displayMode: "inline",
    availableDisplayModes: ["inline"],
    locale: locale.value,
    platform: "desktop",
    toolInfo: { id: callKey.value, tool: { name: info.value?.tool ?? "" } },
  };
}

function maxHeight() {
  return Math.round(window.innerHeight * 0.8);
}

function post(message: unknown) {
  const target = frame.value?.contentWindow;
  if (target && info.value) target.postMessage(message, info.value.origin);
}

function createBridge() {
  bridge?.dispose();
  const app = info.value;
  if (!app) return;
  bridge = new McpAppBridge(post, {
    hostInfo: { name: "Multara", version },
    csp: app.csp,
    clipboardWrite: app.permissions.clipboardWrite,
    toolInput,
    toolResult,
    hostContext,
    call: (method, params) => backend.mcpAppCall(app.serverKey, method, params),
    fillComposer: (text) =>
      fillComposerFromUi({ sessionId: props.sessionId, source: "mcp-app", sourceId: app.serverKey, text }),
    attachContext: (text, data) =>
      canvas.attachContext({
        sessionId: props.sessionId,
        source: "mcp-app",
        sourceId: app.serverKey,
        label: app.server,
        text,
        data,
      }),
    openLink: async (url) => {
      await backend.openExternalUrl(url);
      return true;
    },
    setHeight: (next) => {
      height.value = next;
      heightCache.set(callKey.value, next);
    },
    maxHeight,
  });
}

function onMessage(event: MessageEvent) {
  if (!frame.value || event.source !== frame.value.contentWindow) return;
  if (!info.value || event.origin !== info.value.origin) return;
  void bridge?.handle(event.data);
}

async function resolve() {
  if (resolved.value) return;
  resolved.value = true;
  try {
    info.value = await backend.mcpAppResolve(props.toolUse.tool ?? "");
  } catch (cause) {
    failed.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function load() {
  if (!info.value) return;
  loaded.value = true;
  createBridge();
}

function unload() {
  loaded.value = false;
  bridge?.dispose();
  bridge = null;
}

watch(visible, async (isVisible) => {
  window.clearTimeout(offscreenTimer);
  if (isVisible) {
    await resolve();
    if (!loaded.value) load();
  } else if (loaded.value) {
    offscreenTimer = window.setTimeout(unload, OFFSCREEN_UNLOAD_MS);
  }
});

watch(info, () => {
  if (visible.value && info.value && !loaded.value) load();
});

// 工具完成后推结果
watch(
  () => props.toolResult?.seq,
  () => bridge?.notifyToolResult(),
);

onMounted(async () => {
  version = await backend.appVersion().catch(() => "");
  window.addEventListener("message", onMessage);
  observer = new IntersectionObserver((entries) => {
    visible.value = entries.some((entry) => entry.isIntersecting);
  });
  if (root.value) observer.observe(root.value);
  themeObserver = new MutationObserver(() => bridge?.notifyHostContext());
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
});

onBeforeUnmount(() => {
  window.removeEventListener("message", onMessage);
  window.clearTimeout(offscreenTimer);
  observer?.disconnect();
  themeObserver?.disconnect();
  bridge?.dispose();
});
</script>

<template>
  <!-- 没有界面（工具没声明、或 MCP Apps 关着）：什么都不画 -->
  <div
    v-if="!resolved || info || failed"
    ref="root"
    class="mcp-app"
    :class="{ bordered: info?.prefersBorder !== false, pending: !info && !failed }"
    :style="{ height: info ? `${height}px` : undefined }"
  >
    <iframe
      v-if="info && loaded"
      ref="frame"
      class="mcp-app-frame"
      :src="info.url"
      :title="info.tool"
      sandbox="allow-scripts allow-forms allow-same-origin"
      referrerpolicy="no-referrer"
      :allow="info.permissions.clipboardWrite ? 'clipboard-write' : ''"
    />
    <p v-else-if="failed" class="mcp-app-error">{{ failed }}</p>
  </div>
</template>
