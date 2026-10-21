<script setup lang="ts">
import { computed } from "vue";
import { Bot, Layers } from "@lucide/vue";
import BrandLogo from "./BrandLogo.vue";
import ProviderLogo from "./ProviderLogo.vue";
import { cliIconProvider } from "../domain/cli-catalog";

const props = withDefaults(defineProps<{
  /** 目录 id，如 cursor / multara / all；工作区形如 ws/<id>/cursor */
  owner: string;
  size?: number;
}>(), {
  size: 16,
});

const iconId = computed(() => cliIconProvider(props.owner));
const brandSize = computed(() => Math.max(10, Math.round(props.size * 0.625)));
const boxStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
}));
</script>

<template>
  <!-- 根节点用 span，避免侧栏 .nav-item > svg 的 color 污染品牌色 / ProviderLogo -->
  <span class="cli-source-icon" :class="`is-${iconId}`" :style="boxStyle" aria-hidden="true">
    <Layers v-if="iconId === 'all'" :size="size" />
    <span v-else-if="iconId === 'multara'" class="skill-source-mark">
      <BrandLogo :size="brandSize" />
    </span>
    <Bot v-else-if="iconId === 'agents'" :size="size" />
    <ProviderLogo v-else :provider="iconId" :size="size" />
  </span>
</template>

<style scoped>
.cli-source-icon {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  line-height: 0;
}
.cli-source-icon .skill-source-mark {
  color: #fff;
}
</style>
