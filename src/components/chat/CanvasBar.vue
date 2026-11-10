<script setup lang="ts">
// 输入框上方的 Canvas 栏（和「队列」「已修改的文件」同一排样式）：
// 列出本会话的 Canvas（以及别处固定过来的），点一行显示它，悬停出固定 / 关闭。
import { computed, ref, watch } from "vue";
import { ChevronDown } from "@lucide/vue";
import type { CanvasInstance } from "../../domain/types";
import { useCanvasStore } from "../../stores/canvas";
import { t } from "../../i18n";

const props = defineProps<{ sessionId: string }>();
const canvas = useCanvasStore();
const collapsed = ref(false);
const list = computed(() => canvas.listFor(props.sessionId));

watch(
  () => props.sessionId,
  (sessionId) => {
    if (sessionId) void canvas.refresh(sessionId);
  },
  { immediate: true },
);

// 固定过来的实例属于别的会话：操作一律用它自己的 sessionId
function show(item: CanvasInstance) {
  void canvas.show(item.sessionId, item.instanceId);
}

function togglePin(item: CanvasInstance) {
  void canvas.setPinned(item.sessionId, item.instanceId, !item.pinned);
}

function closeInstance(item: CanvasInstance) {
  void canvas.close(item.sessionId, item.instanceId);
}
</script>

<template>
  <section v-if="list.length" class="chat-queue canvas-bar" :class="{ collapsed }">
    <button type="button" class="chat-queue-heading" :aria-expanded="!collapsed" @click="collapsed = !collapsed">
      <ChevronDown class="chat-queue-chevron" :size="13" />
      <span>{{ t("canvas.barCount", { n: list.length }) }}</span>
    </button>
    <ol v-show="!collapsed" class="chat-queue-list vs-scroll">
      <li
        v-for="item in list"
        :key="`${item.sessionId}/${item.instanceId}`"
        class="chat-queue-item canvas-bar-item"
        :class="{ hidden: !item.visible }"
      >
        <Codicon class="canvas-bar-icon" :name="item.pinned ? 'pinned' : 'dashboard'" :size="13" />
        <button type="button" class="chat-queue-text canvas-bar-open" :title="item.title" @click="show(item)">
          <span class="canvas-bar-title">{{ item.title }}</span>
          <small v-if="item.status" class="canvas-bar-status">{{ item.status }}</small>
          <small v-if="item.sessionId !== sessionId" class="canvas-bar-status">{{ t("canvas.pinnedFromOther") }}</small>
        </button>
        <span class="chat-queue-actions">
          <UiButton
            class="icon-button"
            variant="ghost"
            size="icon"
            :tip="item.pinned ? t('canvas.unpin') : t('canvas.pin')"
            :aria-pressed="Boolean(item.pinned)"
            @click="togglePin(item)"
          >
            <Codicon :name="item.pinned ? 'pinned' : 'pin'" :size="12" />
          </UiButton>
          <UiButton class="icon-button" variant="ghost" size="icon" :tip="t('canvas.endInstance')" @click="closeInstance(item)">
            <Codicon name="trash" :size="12" />
          </UiButton>
        </span>
      </li>
    </ol>
  </section>
</template>
