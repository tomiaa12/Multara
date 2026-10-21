<script setup lang="ts">
/**
 * 品牌字形「叶芽」：两片叶子从同一点生长。
 * 使用 currentColor，放在 .brand-mark（品牌色底板）里显示为白色；右侧叶片降低不透明度形成双色层次。
 * viewBox 以双叶包围盒中心 (512, 568) 为中心裁成 600×600 的正方形，保证小尺寸下图形足够大。
 */
withDefaults(defineProps<{ size?: number }>(), { size: 14 });
</script>

<template>
  <svg :width="size" :height="size" viewBox="212 268 600 600" fill="currentColor" aria-hidden="true">
    <path d="M512 780C392 660 392 420 512 268C632 420 632 660 512 780Z" transform="rotate(-34 512 780)" />
    <path d="M512 780C392 660 392 420 512 268C632 420 632 660 512 780Z" fill-opacity="0.6" transform="rotate(34 512 780)" />
  </svg>
</template>
