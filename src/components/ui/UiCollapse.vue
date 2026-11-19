<script setup lang="ts">
import { computed } from "vue";
import { collapseTransition } from "../../ui/motion";

/**
 * 折叠展开：子元素自己 v-if / v-show，这里只负责高度 0 ↔ auto 的过渡。
 *   <UiCollapse><div v-if="open" class="body">…</div></UiCollapse>
 * 不要包大列表（上千行的树 / 日志）：每帧都要重排后面的内容。
 */
const props = withDefaults(defineProps<{ fade?: boolean; duration?: number }>(), { fade: true });
const hooks = computed(() => collapseTransition({ fade: props.fade, duration: props.duration }));
</script>

<template>
  <Transition v-bind="hooks">
    <slot />
  </Transition>
</template>
