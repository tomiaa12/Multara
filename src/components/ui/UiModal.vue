<script setup lang="ts">
/**
 * 全屏遮罩弹层：Teleport 到 body，避免落在 .vs-scroll（transform）内时
 * position:fixed 相对滚动容器定位，滚动后弹窗跑到页面顶部。
 */
defineOptions({ inheritAttrs: false });
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" v-bind="$attrs">
      <slot />
    </div>
  </Teleport>
</template>
