<script setup lang="ts">
import { computed } from "vue";
import { Icon } from "@iconify/vue/offline";
import { resolveFileIcon } from "../file-icons";

const props = withDefaults(defineProps<{
  name: string;
  directory?: boolean;
  open?: boolean;
  size?: number;
}>(), {
  size: 15,
});

const icon = computed(() =>
  resolveFileIcon(props.name, { directory: props.directory, open: props.open }),
);
</script>

<template>
  <img
    v-if="icon && 'src' in icon"
    class="file-type-icon"
    :src="icon.src"
    :width="size"
    :height="size"
    alt=""
    aria-hidden="true"
    draggable="false"
  />
  <Icon v-else-if="icon" class="file-type-icon" :icon="icon.iconify" :width="size" :height="size" />
  <!-- 主题加载中先占位，避免树节点文字左右跳动 -->
  <span v-else class="file-type-icon" :style="{ width: `${size}px`, height: `${size}px` }" aria-hidden="true" />
</template>
