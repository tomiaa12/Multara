<script setup lang="ts">
import { gitStatusClass } from "./tree-git-status";

defineProps<{
  letter: string;
  /** 文件夹冒泡出来的状态用圆点，文件用字母。 */
  bubble?: boolean;
}>();
</script>

<template>
  <span class="tree-git-badge" :class="[gitStatusClass(letter), { 'is-bubble': bubble }]">{{ bubble ? "" : letter }}</span>
</template>
