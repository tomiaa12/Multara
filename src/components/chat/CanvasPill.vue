<script setup lang="ts">
// 输入框上方的 Canvas 入口（方案 §5.7）：没有实例不显示；1 个显示标题和状态；多个显示数量，点开列表，每项可关闭。
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { CanvasInstance } from "../../domain/types";
import { useCanvasStore } from "../../stores/canvas";
import { t } from "../../i18n";

const props = defineProps<{ sessionId: string }>();
const canvas = useCanvasStore();
const open = ref(false);
const root = ref<HTMLElement | null>(null);

const list = computed(() => canvas.listFor(props.sessionId));
const single = computed(() => (list.value.length === 1 ? list.value[0] : null));

watch(
  () => props.sessionId,
  (sessionId) => {
    open.value = false;
    if (sessionId) void canvas.refresh(sessionId);
  },
  { immediate: true },
);

function onDocumentClick(event: MouseEvent) {
  if (root.value && !root.value.contains(event.target as Node)) open.value = false;
}

watch(open, (value) => {
  if (value) document.addEventListener("mousedown", onDocumentClick);
  else document.removeEventListener("mousedown", onDocumentClick);
});

onBeforeUnmount(() => document.removeEventListener("mousedown", onDocumentClick));

function toggle() {
  if (single.value) {
    void canvas.show(single.value.sessionId, single.value.instanceId);
    return;
  }
  open.value = !open.value;
}

// 固定过来的实例属于别的会话：操作一律用它自己的 sessionId
function pick(item: CanvasInstance) {
  open.value = false;
  void canvas.show(item.sessionId, item.instanceId);
}

function closeInstance(item: CanvasInstance) {
  void canvas.close(item.sessionId, item.instanceId);
}

function togglePin(item: CanvasInstance) {
  void canvas.setPinned(item.sessionId, item.instanceId, !item.pinned);
}
</script>

<template>
  <div v-if="list.length" ref="root" class="canvas-pill-wrap">
    <button
      type="button"
      class="canvas-pill"
      :aria-expanded="single ? undefined : open"
      :aria-label="single ? t('canvas.pillOne', { title: single.title }) : t('canvas.pillMany', { count: list.length })"
      @click="toggle"
    >
      <Codicon :name="single?.pinned ? 'pinned' : 'dashboard'" :size="13" />
      <template v-if="single">
        <span class="canvas-pill-title">{{ single.title }}</span>
        <span v-if="single.status" class="canvas-pill-status">{{ single.status }}</span>
      </template>
      <span v-else>{{ t("canvas.pillMany", { count: list.length }) }}</span>
    </button>
    <ul v-if="open && !single" class="canvas-pill-menu" role="menu">
      <li v-for="item in list" :key="`${item.sessionId}/${item.instanceId}`" role="none">
        <button type="button" role="menuitem" class="canvas-pill-item" @click="pick(item)">
          <span class="canvas-pill-title">{{ item.title }}</span>
          <span v-if="item.status" class="canvas-pill-status">{{ item.status }}</span>
          <span v-if="item.sessionId !== sessionId" class="canvas-pill-status">{{ t("canvas.pinnedFromOther") }}</span>
        </button>
        <UiButton
          size="icon"
          variant="ghost"
          :tip="item.pinned ? t('canvas.unpin') : t('canvas.pin')"
          :aria-label="item.pinned ? t('canvas.unpin') : t('canvas.pin')"
          :aria-pressed="Boolean(item.pinned)"
          @click="togglePin(item)"
        >
          <Codicon :name="item.pinned ? 'pinned' : 'pin'" :size="12" />
        </UiButton>
        <UiButton
          size="icon"
          variant="ghost"
          :tip="t('canvas.closeInstance')"
          :aria-label="t('canvas.closeInstance')"
          @click="closeInstance(item)"
        >
          <Codicon name="close" :size="12" />
        </UiButton>
      </li>
    </ul>
  </div>
</template>
