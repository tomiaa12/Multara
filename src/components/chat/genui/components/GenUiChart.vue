<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { ChartProps } from "../schema";
import {
  CHART_BOX,
  SERIES_COLORS,
  barRects,
  formatNumber,
  labelStride,
  linePath,
  niceTicks,
  pieSlices,
  valueRange,
  xCenter,
  yScale,
} from "../chart-geometry";
import { readGenUiState, writeGenUiState } from "../state";
import { t } from "../../../../i18n";

// GenUiBlock 给所有组件传同一组 props，没声明的不要落到根元素上（例如 title 会变成原生提示）
defineOptions({ inheritAttrs: false });

const props = defineProps<{ data: ChartProps; stateKey: string; title?: string }>();

const showData = ref(readGenUiState<boolean>(props.stateKey, "showData") ?? false);
watch(showData, (value) => writeGenUiState(props.stateKey, "showData", value));

const box = CHART_BOX;
const ticks = computed(() => {
  const range = valueRange(props.data);
  return niceTicks(range.min, range.max);
});
const y = computed(() => yScale(ticks.value));
const stride = computed(() => labelStride(props.data.data.length));
const bars = computed(() => (props.data.chartType === "bar" ? barRects(props.data, ticks.value) : []));
const slices = computed(() => (props.data.chartType === "pie" ? pieSlices(props.data) : []));
const lines = computed(() =>
  props.data.chartType === "line"
    ? props.data.series.map((series, index) => ({
      key: series.key,
      color: SERIES_COLORS[index % SERIES_COLORS.length],
      path: linePath(props.data, series.key, ticks.value),
    }))
    : [],
);
const showDots = computed(() => props.data.data.length <= 60);
const summary = computed(() =>
  t("genui.chartSummary", {
    title: props.title ?? "",
    count: props.data.data.length,
    series: props.data.series.map((item) => item.label).join(", "),
  }),
);

function label(row: number) {
  return String(props.data.data[row]?.[props.data.xKey] ?? "");
}

function tip(row: number, seriesIndex: number) {
  const series = props.data.series[seriesIndex];
  const value = Number(props.data.data[row]?.[series.key]) || 0;
  return `${label(row)} · ${series.label}: ${value.toLocaleString()}${props.data.unit}`;
}

function color(index: number) {
  return SERIES_COLORS[index % SERIES_COLORS.length];
}
</script>

<template>
  <div class="genui-chart">
    <div v-if="props.data.chartType === 'pie'" class="genui-pie">
      <svg viewBox="0 0 240 240" role="img" :aria-label="summary">
        <path
          v-for="slice in slices"
          :key="slice.row"
          :d="slice.path"
          :fill="color(slice.row)"
          class="genui-pie-slice"
        ><title>{{ slice.label }}: {{ slice.value.toLocaleString() }}{{ props.data.unit }} ({{ (slice.share * 100).toFixed(1) }}%)</title></path>
      </svg>
      <ul class="genui-legend">
        <li v-for="slice in slices" :key="slice.row">
          <i :style="{ background: color(slice.row) }" />{{ slice.label }}
          <span>{{ (slice.share * 100).toFixed(1) }}%</span>
        </li>
      </ul>
    </div>
    <template v-else>
      <svg :viewBox="`0 0 ${box.width} ${box.height}`" role="img" :aria-label="summary" class="genui-xy">
        <g class="genui-grid">
          <template v-for="tick in ticks" :key="tick">
            <line :x1="box.left" :x2="box.width - box.right" :y1="y(tick)" :y2="y(tick)" :class="{ zero: tick === 0 }" />
            <text :x="box.left - 6" :y="y(tick)" text-anchor="end" dominant-baseline="middle">{{ formatNumber(tick) }}</text>
          </template>
          <template v-for="(_, row) in props.data.data" :key="row">
            <text
              v-if="row % stride === 0"
              :x="xCenter(row, props.data.data.length)"
              :y="box.height - 8"
              text-anchor="middle"
            >{{ label(row) }}</text>
          </template>
        </g>
        <g v-if="props.data.chartType === 'bar'">
          <rect
            v-for="bar in bars"
            :key="`${bar.row}-${bar.series}`"
            :x="bar.x"
            :y="bar.y"
            :width="bar.width"
            :height="bar.height"
            :fill="color(bar.series)"
            rx="1.5"
          ><title>{{ tip(bar.row, bar.series) }}</title></rect>
        </g>
        <g v-else>
          <g v-for="(line, seriesIndex) in lines" :key="line.key">
            <path :d="line.path" fill="none" :stroke="line.color" stroke-width="2" stroke-linejoin="round" />
            <template v-if="showDots">
              <circle
                v-for="(row, rowIndex) in props.data.data"
                :key="rowIndex"
                :cx="xCenter(rowIndex, props.data.data.length)"
                :cy="y(Number(row[line.key]) || 0)"
                r="3"
                :fill="line.color"
              ><title>{{ tip(rowIndex, seriesIndex) }}</title></circle>
            </template>
          </g>
        </g>
      </svg>
      <ul v-if="props.data.series.length > 1" class="genui-legend">
        <li v-for="(series, index) in props.data.series" :key="series.key">
          <i :style="{ background: color(index) }" />{{ series.label }}
        </li>
      </ul>
    </template>
    <div class="genui-toolbar">
      <UiButton size="small" variant="ghost" :aria-expanded="showData" @click="showData = !showData">
        {{ showData ? t("genui.hideData") : t("genui.showData") }}
      </UiButton>
    </div>
    <div v-if="showData" class="genui-table genui-chart-data">
      <div class="genui-table-scroll vs-scroll">
        <table>
          <thead>
            <tr>
              <th>{{ props.data.xKey }}</th>
              <th v-for="series in props.data.series" :key="series.key" class="right">{{ series.label }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, index) in props.data.data" :key="index">
              <td>{{ row[props.data.xKey] }}</td>
              <td v-for="series in props.data.series" :key="series.key" class="right">
                {{ Number(row[series.key]).toLocaleString() }}{{ props.data.unit }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
