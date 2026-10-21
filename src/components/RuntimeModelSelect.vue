<script setup lang="ts">
/*
 * 智能体的模型选择：能从运行时 CLI 查到模型列表时用可搜索下拉框，同时允许输入列表外的模型名；
 * 查不到（或该运行时只能手填）时退回输入框；运行时不接收 --model 时禁用并说明。
 */
import { computed, ref, watch } from "vue";
import { RefreshCw } from "@lucide/vue";
import { listRuntimeModels, type RuntimeModels } from "../services/backend";
import type { SelectOption } from "./ui/select-types";

const props = defineProps<{ runtimeId: string }>();
const model = defineModel<string>({ required: true });

const info = ref<RuntimeModels | null>(null);
const loading = ref(false);
let requestId = 0;

async function load(refresh = false) {
  const runtimeId = props.runtimeId;
  const current = ++requestId;
  if (!runtimeId) {
    info.value = null;
    return;
  }
  loading.value = true;
  try {
    const result = await listRuntimeModels(runtimeId, refresh);
    if (current === requestId) info.value = result;
  } catch (cause) {
    if (current === requestId) {
      info.value = { supported: true, source: "manual", models: [], error: cause instanceof Error ? cause.message : String(cause) };
    }
  } finally {
    if (current === requestId) loading.value = false;
  }
}

watch(() => props.runtimeId, () => void load(), { immediate: true });

const options = computed<SelectOption[]>(() => {
  const list: SelectOption[] = (info.value?.models ?? []).map((item) => ({
    value: item.id,
    label: item.label,
    meta: item.label === item.id ? undefined : item.id,
    keywords: [item.id],
  }));
  // 「auto」表示交给运行时默认；CLI 列表里没有时补上，保证总能选回默认
  if (!list.some((item) => item.value === "auto")) list.unshift({ value: "auto", label: "Auto", meta: "运行时默认" });
  // 手填过的模型不在列表里时也要能显示出来
  if (model.value && !list.some((item) => item.value === model.value)) {
    list.push({ value: model.value, label: model.value, meta: "自定义" });
  }
  return list;
});

const useSelect = computed(() => Boolean(info.value?.supported && info.value.models.length));
const selectValue = computed({
  get: () => model.value || "auto",
  set: (value: string | null) => { model.value = value || "auto"; },
});
</script>

<template>
  <div class="runtime-model-select">
    <div class="runtime-model-control">
      <UiSelect
        v-if="useSelect"
        v-model="selectValue"
        :options="options"
        searchable
        creatable
        search-placeholder="搜索或输入模型名"
      />
      <UiInput
        v-else
        v-model="model"
        :disabled="info?.supported === false"
        :placeholder="loading ? '正在读取模型列表…' : 'auto'"
      />
      <UiButton
        v-if="info?.source === 'cli' || info?.error"
        variant="ghost"
        size="icon"
        type="button"
        :class="{ 'is-loading': loading }"
        :disabled="loading"
        title="重新读取模型列表"
        @click="load(true)"
      >
        <RefreshCw :size="14" />
      </UiButton>
    </div>
    <small v-if="info?.supported === false" class="runtime-model-hint">这个运行时不支持指定模型，会使用它自己的默认设置。</small>
    <small v-else-if="info?.error" class="runtime-model-hint is-warn">没读到模型列表：{{ info.error }}。可以直接填写模型名。</small>
    <small v-else-if="info?.source === 'preset'" class="runtime-model-hint">这些是官方别名，始终指向该系列的最新版本；也可以在搜索框里输入完整模型名。</small>
    <small v-else-if="info?.source === 'manual' && info.supported" class="runtime-model-hint">这个运行时不能列出模型，请直接填写模型名，留空或 auto 使用默认。</small>
  </div>
</template>
