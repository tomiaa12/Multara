<script setup lang="ts">
import { computed, useAttrs } from "vue";

/**
 * 全屏遮罩弹层：Teleport 到 body，避免落在 .vs-scroll（transform）内时
 * position:fixed 相对滚动容器定位，滚动后弹窗跑到页面顶部。
 */
defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{ closeOnBackdrop?: boolean }>(), {
  closeOnBackdrop: false,
});
const attrs = useAttrs();
const backdropAttrs = computed(() =>
  props.closeOnBackdrop
    ? attrs
    : Object.fromEntries(
        Object.entries(attrs).filter(
          ([key]) => key !== "onClick" && key !== "onMousedown",
        ),
      ),
);
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" v-bind="backdropAttrs">
      <slot />
    </div>
  </Teleport>
</template>
