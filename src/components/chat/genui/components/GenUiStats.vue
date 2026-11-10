<script setup lang="ts">
import type { StatsProps } from "../schema";
import { t } from "../../../../i18n";

// GenUiBlock 给所有组件传同一组 props，没声明的不要落到根元素上（例如 title 会变成原生提示）
defineOptions({ inheritAttrs: false });

const props = defineProps<{ data: StatsProps; stateKey: string }>();

function deltaText(delta: number) {
  const sign = delta > 0 ? "+" : "";
  return `${sign}${Number.isInteger(delta) ? delta : delta.toFixed(2).replace(/\.?0+$/, "")}`;
}
</script>

<template>
  <ul class="genui-stats">
    <li v-for="(item, index) in props.data.items" :key="index" class="genui-stat">
      <span class="genui-stat-label">{{ item.label }}</span>
      <strong class="genui-stat-value">{{ item.value }}<small v-if="item.unit">{{ item.unit }}</small></strong>
      <span
        v-if="item.delta !== null"
        class="genui-stat-delta"
        :data-trend="item.delta > 0 ? 'up' : item.delta < 0 ? 'down' : 'flat'"
        :aria-label="item.delta > 0 ? t('genui.trendUp') : item.delta < 0 ? t('genui.trendDown') : t('genui.trendFlat')"
      >{{ item.delta > 0 ? "▲" : item.delta < 0 ? "▼" : "–" }} {{ deltaText(item.delta) }}</span>
    </li>
  </ul>
</template>
