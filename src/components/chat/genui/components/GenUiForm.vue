<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { initialFormValue, missingRequired, type FormField, type FormProps } from "../schema";
import { readGenUiState, writeGenUiState } from "../state";
import type { GenUiSubmission } from "../../ui-outbound";
import { isRecord } from "../block";
import { t } from "../../../../i18n";

// GenUiBlock 给所有组件传同一组 props，没声明的不要落到根元素上（例如 title 会变成原生提示）
defineOptions({ inheritAttrs: false });

const props = defineProps<{
  data: FormProps;
  stateKey: string;
  submitted?: GenUiSubmission | null;
  disabled?: boolean;
  disabledReason?: string;
}>();
const emit = defineEmits<{ action: [action: string, values?: unknown] }>();

function initialValues(): Record<string, unknown> {
  const sent = props.submitted?.values;
  const draft = readGenUiState<Record<string, unknown>>(props.stateKey, "draft");
  return Object.fromEntries(props.data.fields.map((field) => {
    if (isRecord(sent) && field.name in sent) return [field.name, sent[field.name]];
    if (draft && field.name in draft) return [field.name, draft[field.name]];
    return [field.name, initialFormValue(field)];
  }));
}

const values = reactive<Record<string, unknown>>(initialValues());
const missing = ref<string | null>(null);
const readonly = computed(() => Boolean(props.submitted));

watch(values, (next) => {
  if (readonly.value) return;
  writeGenUiState(props.stateKey, "draft", { ...next });
  if (missing.value && !missingRequired(props.data.fields, next)) missing.value = null;
}, { deep: true });

// 提交回来后（或换了会话重建时）用已提交的值覆盖草稿
watch(() => props.submitted, (sent) => {
  if (!sent || !isRecord(sent.values)) return;
  Object.assign(values, sent.values);
});

function options(field: FormField) {
  return field.options.map((option) => ({ value: option.value, label: option.label }));
}

function submit() {
  if (readonly.value || props.disabled) return;
  const absent = missingRequired(props.data.fields, values);
  if (absent) {
    missing.value = absent;
    return;
  }
  emit("action", "submit", { ...values });
}
</script>

<template>
  <form class="genui-form" :class="{ readonly }" novalidate @submit.prevent="submit">
    <div
      v-for="field in props.data.fields"
      :key="field.name"
      class="genui-field"
      :class="[`kind-${field.kind}`, { invalid: missing === field.name }]"
    >
      <label :for="`${stateKey}:${field.name}`">
        {{ field.label }}<span v-if="field.required" class="genui-required" aria-hidden="true">*</span>
      </label>
      <UiTextarea
        v-if="field.kind === 'textarea'"
        :id="`${stateKey}:${field.name}`"
        :model-value="String(values[field.name] ?? '')"
        :placeholder="field.placeholder"
        :disabled="readonly"
        rows="3"
        @update:model-value="values[field.name] = $event"
      />
      <UiSelect
        v-else-if="field.kind === 'select' || field.kind === 'multiselect'"
        :id="`${stateKey}:${field.name}`"
        :model-value="(values[field.name] as string | string[])"
        @update:model-value="values[field.name] = $event"
        :options="options(field)"
        :multiple="field.kind === 'multiselect'"
        :placeholder="field.placeholder || t('genui.selectPlaceholder')"
        :disabled="readonly"
      />
      <span v-else-if="field.kind === 'slider'" class="genui-slider">
        <input
          :id="`${stateKey}:${field.name}`"
          v-model.number="values[field.name]"
          type="range"
          :min="field.min ?? 0"
          :max="field.max ?? 100"
          :step="field.step ?? 1"
          :disabled="readonly"
        />
        <output>{{ values[field.name] }}</output>
      </span>
      <UiInput
        v-else-if="field.kind === 'switch'"
        :id="`${stateKey}:${field.name}`"
        v-model="values[field.name]"
        class="ds-switch"
        type="checkbox"
        :disabled="readonly"
      />
      <UiInput
        v-else-if="field.kind === 'number'"
        :id="`${stateKey}:${field.name}`"
        v-model.number="values[field.name]"
        type="number"
        :min="field.min ?? undefined"
        :max="field.max ?? undefined"
        :step="field.step ?? undefined"
        :placeholder="field.placeholder"
        :disabled="readonly"
      />
      <UiInput
        v-else
        :id="`${stateKey}:${field.name}`"
        v-model="values[field.name]"
        :type="field.kind === 'date' ? 'date' : 'text'"
        :placeholder="field.placeholder"
        :disabled="readonly"
      />
    </div>
    <div class="genui-actions">
      <span v-if="readonly" class="genui-submitted">{{ t("genui.submitted") }}</span>
      <template v-else>
        <span v-if="missing" class="genui-error" role="alert">{{ t("genui.requiredMissing") }}</span>
        <UiButton
          type="submit"
          size="small"
          variant="primary"
          :disabled="disabled"
          :tip="disabled ? disabledReason : undefined"
        >{{ props.data.submitLabel || t("genui.submit") }}</UiButton>
      </template>
    </div>
  </form>
</template>
