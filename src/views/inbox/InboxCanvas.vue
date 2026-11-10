<script setup lang="ts">
// 收件箱右侧的 Canvas 模块（方案 §5.7）：列出本会话的实例，选中一个显示。
// 和工作台、聊天弹窗共用 UiFrame：同一实例同时只在一个地方活着，别处显示「在此显示」。
import { computed, ref, watch } from "vue";
import UiFrame from "../../components/UiFrame.vue";
import { useCanvasStore } from "../../stores/canvas";
import { t } from "../../i18n";

const props = defineProps<{ sessionId: string }>();
const canvas = useCanvasStore();
const viewId = `inbox-${crypto.randomUUID()}`;
const selected = ref("");

const list = computed(() => canvas.listFor(props.sessionId));
const current = computed(
  () => list.value.find((item) => item.instanceId === selected.value) ?? list.value[0] ?? null,
);

watch(
  () => props.sessionId,
  (sessionId) => {
    selected.value = "";
    canvas.startListening();
    if (sessionId) void canvas.refresh(sessionId);
  },
  { immediate: true },
);

// pill / 菜单要显示本会话的某个实例时，切过去
watch(
  () => canvas.showRequest?.nonce,
  () => {
    const request = canvas.showRequest;
    if (request && canvas.shownIn(props.sessionId, request.sessionId, request.instanceId)) {
      selected.value = request.instanceId;
    }
  },
);

function closeCurrent() {
  const target = current.value;
  if (!target) return;
  selected.value = "";
  void canvas.close(target.sessionId, target.instanceId);
}
</script>

<template>
  <div class="inbox-canvas">
    <p v-if="!list.length" class="inbox-canvas-empty">{{ t("canvas.inboxEmpty") }}</p>
    <template v-else>
      <div v-if="list.length > 1" class="inbox-canvas-switch" role="tablist">
        <button
          v-for="item in list"
          :key="item.instanceId"
          type="button"
          role="tab"
          :aria-selected="current?.instanceId === item.instanceId"
          class="inbox-canvas-option"
          @click="selected = item.instanceId"
        >
          {{ item.title }}
        </button>
      </div>
      <UiFrame
        v-if="current"
        :key="`${current.sessionId}/${current.instanceId}`"
        :session-id="current.sessionId"
        :instance-id="current.instanceId"
        :title="current.title"
        :view-id="viewId"
        @close="closeCurrent"
      />
    </template>
  </div>
</template>

<style scoped>
.inbox-canvas {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.inbox-canvas-empty {
  margin: 0;
  padding: 16px;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}
.inbox-canvas-switch {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--color-border);
}
.inbox-canvas-option {
  padding: 2px 8px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-full);
  background: none;
  color: var(--color-text-secondary);
  font: inherit;
  font-size: var(--font-size-xs);
  cursor: pointer;
}
.inbox-canvas-option[aria-selected="true"] {
  border-color: var(--color-primary-border);
  background: var(--color-primary-subtle);
  color: var(--color-text);
}
</style>
