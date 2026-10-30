<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch } from "vue";
import {
  chatMessagesKey,
  estimateContextUsage,
  formatTokenCount,
} from "./context-usage";

const props = withDefaults(defineProps<{
  placement?: "up" | "down";
}>(), {
  placement: "up",
});

const messages = inject(chatMessagesKey, null);
const open = ref(false);
const root = ref<HTMLElement | null>(null);
const usage = computed(() => estimateContextUsage(messages?.value ?? []));
const rounded = computed(() => Math.min(100, Math.round(usage.value.percent)));
const radius = 6;
const circumference = 2 * Math.PI * radius;
const dashOffset = computed(() =>
  circumference * (1 - Math.min(100, usage.value.percent) / 100),
);
const tone = computed(() => {
  if (usage.value.percent >= 90) return "error";
  if (usage.value.percent >= 75) return "warning";
  return "";
});
const barTotal = computed(() => usage.value.slices.reduce((sum, slice) => sum + slice.tokens, 0));

function onDocumentPointer(event: PointerEvent) {
  if (!open.value) return;
  const target = event.target as Node | null;
  if (target && root.value?.contains(target)) return;
  open.value = false;
}

watch(open, (value) => {
  if (value) document.addEventListener("pointerdown", onDocumentPointer, true);
  else document.removeEventListener("pointerdown", onDocumentPointer, true);
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onDocumentPointer, true);
});
</script>

<template>
  <div ref="root" class="context-usage" :class="tone">
    <UiTooltip :content="`上下文用量 ${rounded}%`"><button
      type="button"
      class="context-usage-button"
     
      :aria-expanded="open"
      @click.stop="open = !open"
    >
      <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <circle cx="8" cy="8" :r="radius" fill="none" style="stroke: var(--color-border-strong)" stroke-width="2" />
        <circle
          class="context-usage-arc"
          cx="8"
          cy="8"
          :r="radius"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          :stroke-dasharray="circumference"
          :stroke-dashoffset="dashOffset"
          transform="rotate(-90 8 8)"
        />
      </svg>
    </button></UiTooltip>
    <div
      v-if="open"
      class="context-usage-popover"
      :class="props.placement === 'down' ? 'is-down' : 'is-up'"
      role="dialog"
      aria-label="上下文用量"
    >
      <header>
        <strong>上下文用量</strong>
        <span>{{ rounded }}%</span>
      </header>
      <div class="context-usage-bar" aria-hidden="true">
        <span
          v-for="slice in usage.slices"
          :key="slice.id"
          :style="{
            width: `${barTotal ? (slice.tokens / barTotal) * 100 : 0}%`,
            background: slice.color,
          }"
        />
      </div>
      <p class="context-usage-total">~{{ formatTokenCount(usage.used) }} / {{ formatTokenCount(usage.window) }} Tokens</p>
      <ul>
        <li v-for="slice in usage.slices" :key="slice.id">
          <i :style="{ background: slice.color }" />
          <span>{{ slice.label }}</span>
          <b>{{ formatTokenCount(slice.tokens) }}</b>
        </li>
      </ul>
      <p class="context-usage-note">按文本长度估算，约 4 个字符计 1 token。模型没有回传精确用量。</p>
    </div>
  </div>
</template>
