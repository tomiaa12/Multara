<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { Cell, ColumnFormat, TableProps } from "../schema";
import { readGenUiState, writeGenUiState } from "../state";
import { t } from "../../../../i18n";

// GenUiBlock 给所有组件传同一组 props，没声明的不要落到根元素上（例如 title 会变成原生提示）
defineOptions({ inheritAttrs: false });

const props = defineProps<{ data: TableProps; stateKey: string }>();

type Sort = { key: string; dir: "asc" | "desc" } | null;
const sort = ref<Sort>(readGenUiState<Sort>(props.stateKey, "sort") ?? null);
watch(sort, (value) => writeGenUiState(props.stateKey, "sort", value));
const copied = ref<"" | "tsv" | "md">("");

const rows = computed(() => {
  const current = sort.value;
  if (!current) return props.data.rows;
  const factor = current.dir === "asc" ? 1 : -1;
  return [...props.data.rows].sort((a, b) => compare(a[current.key], b[current.key]) * factor);
});

function compare(a: Cell, b: Cell) {
  if (a === b) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  if (typeof a === "number" && typeof b === "number") return a - b;
  return String(a).localeCompare(String(b), undefined, { numeric: true });
}

function toggleSort(key: string) {
  const current = sort.value;
  if (!current || current.key !== key) sort.value = { key, dir: "asc" };
  else if (current.dir === "asc") sort.value = { key, dir: "desc" };
  else sort.value = null;
}

function display(value: Cell, format: ColumnFormat) {
  if (value === null) return "";
  if (format === "number" && typeof value === "number") return value.toLocaleString();
  if (format === "percent" && typeof value === "number") {
    // 0–1 视为比例，其余视为已是百分数
    const percent = Math.abs(value) <= 1 ? value * 100 : value;
    return `${Number(percent.toFixed(2))}%`;
  }
  if (format === "date" && (typeof value === "string" || typeof value === "number")) {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleDateString();
  }
  return String(value);
}

function plain(value: Cell) {
  return value === null ? "" : String(value).replace(/[\t\r\n]+/g, " ");
}

function asTsv() {
  const head = props.data.columns.map((column) => plain(column.label)).join("\t");
  const body = rows.value.map((row) => props.data.columns.map((column) => plain(row[column.key])).join("\t"));
  return [head, ...body].join("\n");
}

function asMarkdown() {
  const escape = (value: Cell) => plain(value).replace(/\|/g, "\\|");
  const head = `| ${props.data.columns.map((column) => escape(column.label)).join(" | ")} |`;
  const rule = `| ${props.data.columns.map((column) => (column.align === "right" ? "---:" : "---")).join(" | ")} |`;
  const body = rows.value.map((row) => `| ${props.data.columns.map((column) => escape(row[column.key])).join(" | ")} |`);
  return [head, rule, ...body].join("\n");
}

async function copy(kind: "tsv" | "md") {
  await navigator.clipboard.writeText(kind === "tsv" ? asTsv() : asMarkdown());
  copied.value = kind;
  window.setTimeout(() => (copied.value = ""), 1200);
}
</script>

<template>
  <div class="genui-table">
    <div class="genui-table-scroll vs-scroll">
      <table>
        <thead>
          <tr>
            <th
              v-for="column in props.data.columns"
              :key="column.key"
              :class="{ right: column.align === 'right' }"
              :aria-sort="sort?.key === column.key ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none'"
            >
              <button type="button" class="genui-sort" @click="toggleSort(column.key)">
                {{ column.label }}<span v-if="sort?.key === column.key" aria-hidden="true">{{ sort.dir === "asc" ? " ↑" : " ↓" }}</span>
              </button>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, index) in rows" :key="index">
            <td v-for="column in props.data.columns" :key="column.key" :class="{ right: column.align === 'right' }">
              {{ display(row[column.key], column.format) }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="genui-toolbar">
      <UiButton size="small" variant="ghost" @click="copy('tsv')">{{ copied === "tsv" ? t("chat.copied") : t("genui.copyTsv") }}</UiButton>
      <UiButton size="small" variant="ghost" @click="copy('md')">{{ copied === "md" ? t("chat.copied") : t("genui.copyMarkdown") }}</UiButton>
    </div>
  </div>
</template>
