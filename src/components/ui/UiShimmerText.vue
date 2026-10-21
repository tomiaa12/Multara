<script setup lang="ts">
// 公共文字效果：active 时从左到右扫光；rotate 时换文案从下往上轮播。样式在 design/components.css。
withDefaults(defineProps<{
  text: string;
  active?: boolean;
  rotate?: boolean;
  tag?: string;
}>(), {
  active: true,
  rotate: false,
  tag: "span",
});

// 和 --motion-duration-normal 一致。扫光是无限动画，必须显式给时长，否则 Transition 会等到扫光一圈结束才移除旧文案
const ROTATE_DURATION = 180;
</script>

<template>
  <component :is="tag" v-if="rotate" class="ds-text-rotate">
    <Transition name="ds-text-rotate" :duration="ROTATE_DURATION">
      <span :key="text" :class="{ 'ds-text-shimmer': active }">{{ text }}</span>
    </Transition>
  </component>
  <component :is="tag" v-else :class="{ 'ds-text-shimmer': active }">{{ text }}</component>
</template>
