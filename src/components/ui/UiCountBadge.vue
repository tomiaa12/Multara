<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { countBadgeDigitsAttr, formatCountBadge, resolveCountBadgeLabel } from "../../ui/count-badge";

const props = withDefaults(defineProps<{
  /** 原始计数；大于 99 显示 99+ */
  count?: number | null;
  /** 已有文案或数字；与 count 同时存在时优先 label */
  label?: string | number | null;
  tone?: "default" | "primary";
}>(), {
  count: null,
  label: null,
  tone: "default",
});

const text = computed(() => {
  if (props.label != null && props.label !== "") {
    return resolveCountBadgeLabel(props.label);
  }
  if (props.count == null) return "";
  return formatCountBadge(props.count);
});

const digits = computed(() => countBadgeDigitsAttr(text.value));

/* 出现 / 消失走 ds-scale；数字变了轻跳一下（重挂 ds-bump 类重新触发动画） */
const badgeEl = ref<HTMLElement | null>(null);
watch(text, (next, previous) => {
  const el = badgeEl.value;
  if (!el || !next || !previous || next === previous) return;
  el.classList.remove("ds-bump");
  void el.offsetWidth;
  el.classList.add("ds-bump");
});
</script>

<template>
  <Transition name="ds-scale">
    <span
      v-if="text"
      ref="badgeEl"
      class="ds-count"
      :data-tone="tone === 'primary' ? 'primary' : undefined"
      :data-digits="digits"
      @animationend="($event.target as HTMLElement).classList.remove('ds-bump')"
    >{{ text }}</span>
  </Transition>
</template>
