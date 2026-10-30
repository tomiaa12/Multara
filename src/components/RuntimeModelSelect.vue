<script setup lang="ts">
/*
 * 智能体的模型选择：能从运行时 CLI 查到模型列表时用可搜索下拉框，同时允许输入列表外的模型名；
 * 查不到（或该运行时只能手填）时退回输入框；运行时不接收 --model 时禁用并说明。
 */
import { computed, ref, watch } from "vue";
import { RefreshCw } from "@lucide/vue";
import type { RuntimeModels } from "../services/backend";
import { requestRuntimeModels } from "./runtime-models-request";
import type { SelectOption } from "./ui/select-types";
import { t } from "../i18n";

const props = withDefaults(defineProps<{
  runtimeId: string;
  compact?: boolean;
  size?: "sm" | "md" | "lg";
}>(), {
  compact: false,
  size: "md",
});
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
    const result = await requestRuntimeModels(runtimeId, refresh);
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
  if (!list.some((item) => item.value === "auto")) list.unshift({ value: "auto", label: "Auto", meta: t("runtimeModel.metaDefault") });
  // 手填过的模型不在列表里时也要能显示出来
  if (model.value && !list.some((item) => item.value === model.value)) {
    list.push({ value: model.value, label: model.value, meta: t("runtimeModel.metaCustom") });
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
  <div class="runtime-model-select" :class="{ 'is-compact': compact }">
    <div class="runtime-model-control">
      <UiSelect
        v-if="useSelect"
        v-model="selectValue"
        :options="options"
        :size="size"
        searchable
        creatable
        :search-placeholder="t('runtimeModel.searchPlaceholder')"
      />
      <UiInput
        v-else
        v-model="model"
        :disabled="info?.supported === false"
        :placeholder="loading ? t('runtimeModel.loadingList') : 'auto'"
      />
      <UiButton
        v-if="!compact && (info?.source === 'cli' || info?.error)"
        variant="ghost"
        size="icon"
        type="button"
        :class="{ 'is-loading': loading }"
        :disabled="loading"
        :tip="t('runtimeModel.refreshList')"
        @click="load(true)"
      >
        <RefreshCw :size="14" />
      </UiButton>
    </div>
    <template v-if="!compact">
      <small v-if="info?.supported === false" class="runtime-model-hint">{{ t("runtimeModel.unsupportedHint") }}</small>
      <small v-else-if="info?.error" class="runtime-model-hint is-warn">{{ t("runtimeModel.listErrorHint", { error: info.error }) }}</small>
      <small v-else-if="info?.source === 'preset'" class="runtime-model-hint">{{ t("runtimeModel.presetHint") }}</small>
      <small v-else-if="info?.source === 'manual' && info.supported" class="runtime-model-hint">{{ t("runtimeModel.manualHint") }}</small>
    </template>
  </div>
</template>
