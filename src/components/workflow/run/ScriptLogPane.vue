<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { t } from "../../../i18n";
import * as backend from "../../../services/backend";
import { useWorkflowsStore } from "../../../stores/workflows";

const props = defineProps<{
  nodeRunId: string;
  /** 节点运行还在进行：1 秒轮询增量日志（live 事件只用来触发立即拉取） */
  live: boolean;
}>();

const store = useWorkflowsStore();
const text = ref("");
const loading = ref(true);
const error = ref("");
const box = ref<HTMLElement | null>(null);
let offset = 0;
let stick = true;
let timer = 0;
let token = 0;
let pulling = false;

async function pull() {
  if (pulling) return;
  pulling = true;
  const mine = token;
  try {
    for (;;) {
      const chunk = await backend.getWorkflowNodeLog(props.nodeRunId, offset);
      if (mine !== token) return;
      if (chunk.text) text.value += chunk.text;
      offset = chunk.nextOffset;
      if (chunk.eof || !chunk.text) break;
    }
    error.value = "";
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    pulling = false;
  }
}

async function reset() {
  token += 1;
  window.clearInterval(timer);
  text.value = "";
  offset = 0;
  loading.value = true;
  store.clearLiveLog(props.nodeRunId);
  await pull();
  loading.value = false;
  if (props.live) timer = window.setInterval(() => void pull(), 1000);
}

onMounted(reset);
onBeforeUnmount(() => {
  token += 1;
  window.clearInterval(timer);
});
watch(() => props.nodeRunId, reset);
watch(() => props.live, (live) => {
  window.clearInterval(timer);
  if (live) timer = window.setInterval(() => void pull(), 1000);
  else void pull();
});
// live 事件到达时立即拉一次，不直接拼接 chunk，避免和文件内容重复
watch(() => store.liveLogs[props.nodeRunId], () => {
  if (props.live) void pull();
});

watch(text, async () => {
  await nextTick();
  if (stick && box.value) box.value.scrollTop = box.value.scrollHeight;
});

function onScroll() {
  const el = box.value;
  if (!el) return;
  stick = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
}
</script>

<template>
  <div class="script-log">
    <div v-if="loading" class="log-hint">{{ t("workflow.drawer.logLoading") }}</div>
    <div v-else-if="error" class="log-hint log-error">{{ error }}</div>
    <div v-else-if="!text" class="log-hint">{{ live ? t("workflow.drawer.logWaiting") : t("workflow.drawer.logEmpty") }}</div>
    <pre v-if="text" ref="box" class="log-pre" @scroll="onScroll">{{ text }}</pre>
  </div>
</template>

<style scoped>
.script-log {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
}
.log-hint {
  color: var(--color-text-tertiary);
  font-size: var(--font-size-sm);
}
.log-error {
  color: var(--color-error);
}
.log-pre {
  flex: 1;
  min-height: 160px;
  margin: 0;
  overflow: auto;
  padding: var(--spacing-2) var(--spacing-3);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg-secondary);
  color: var(--color-text);
  font-family: var(--font-family-mono);
  font-size: var(--font-size-sm);
  line-height: var(--line-height-normal);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>
