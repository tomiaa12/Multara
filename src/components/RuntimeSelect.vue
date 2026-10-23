<script setup lang="ts">
/*
 * 公共运行时下拉：在线运行时列表，选项展示 ProviderLogo（与智能体头像位一致）。
 */
import { computed } from "vue";
import { useAppStore } from "../stores/app";
import type { SelectOption } from "./ui/select-types";
import ProviderLogo from "./ProviderLogo.vue";

const model = defineModel<string>({ default: "" });
const props = withDefaults(defineProps<{
  onlineOnly?: boolean;
  required?: boolean;
  disabled?: boolean;
  placeholder?: string;
  clearable?: boolean;
  size?: "sm" | "md" | "lg";
  variant?: "default" | "ghost";
}>(), {
  onlineOnly: true,
  required: false,
  disabled: false,
  placeholder: "选择运行时",
  clearable: false,
  size: "md",
  variant: "default",
});

const store = useAppStore();
const runtimes = computed(() => {
  const list = store.data?.runtimes ?? [];
  return props.onlineOnly ? list.filter((item) => item.status === "online") : list;
});
const providerById = computed(() =>
  Object.fromEntries(runtimes.value.map((item) => [item.id, item.provider])),
);
const options = computed<SelectOption[]>(() =>
  runtimes.value.map((item) => ({
    value: item.id,
    label: item.name,
    description: item.version || undefined,
    keywords: [item.provider, item.command ?? ""],
  })),
);

function providerOf(option?: SelectOption | null) {
  if (!option) return "";
  return providerById.value[String(option.value)] ?? "";
}
</script>

<template>
  <UiSelect
    v-model="model"
    :options="options"
    :required="required"
    :disabled="disabled || !options.length"
    :placeholder="options.length ? placeholder : '没有可用运行时'"
    :clearable="clearable"
    :size="size"
    :variant="variant"
  >
    <template #option-leading="{ option }">
      <ProviderLogo v-if="option" class="runtime-select-logo" :provider="providerOf(option)" :size="18" />
    </template>
  </UiSelect>
</template>
