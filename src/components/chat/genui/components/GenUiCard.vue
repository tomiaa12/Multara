<script setup lang="ts">
import { computed } from "vue";
import type { CardProps } from "../schema";
import { useAppStore } from "../../../../stores/app";
import { useCanvasStore } from "../../../../stores/canvas";
import type { GenUiSubmission } from "../../ui-outbound";
import { t } from "../../../../i18n";

// GenUiBlock 给所有组件传同一组 props，没声明的不要落到根元素上（例如 title 会变成原生提示）
defineOptions({ inheritAttrs: false });

const props = defineProps<{
  data: CardProps;
  stateKey: string;
  submitted?: GenUiSubmission | null;
  disabled?: boolean;
  disabledReason?: string;
  sessionId?: string;
}>();
const emit = defineEmits<{ action: [action: string, values?: unknown] }>();
const store = useAppStore();
const canvas = useCanvasStore();
const canvasOn = computed(
  () => store.data?.settings.genuiEnabled === "true" && store.data?.settings.canvasesEnabled === "true",
);
/** openCanvas 按钮在 Canvas 关闭时隐藏（不是置灰） */
const actions = computed(() => props.data.actions.filter((action) => !action.openCanvas || canvasOn.value));
const submitActions = computed(() => actions.value.filter((action) => !action.openCanvas));

function click(action: CardProps["actions"][number]) {
  if (action.openCanvas) {
    if (props.sessionId) void canvas.openRegistered(props.sessionId, action.openCanvas);
    return;
  }
  emit("action", action.id);
}

const pickedLabel = computed(() => {
  const id = props.submitted?.action;
  if (!id) return "";
  return props.data.actions.find((item) => item.id === id)?.label ?? id;
});
</script>

<template>
  <div class="genui-card">
    <dl v-if="props.data.fields.length" class="genui-card-fields">
      <template v-for="(field, index) in props.data.fields" :key="index">
        <dt>{{ field.label }}</dt>
        <dd>{{ field.value }}</dd>
      </template>
    </dl>
    <p v-if="props.data.body" class="genui-card-body">{{ props.data.body }}</p>
    <div v-if="actions.length" class="genui-actions">
      <span v-if="submitted && submitActions.length" class="genui-submitted">{{ t("genui.picked", { label: pickedLabel }) }}</span>
      <template v-for="action in actions" :key="action.id">
        <!-- 打开 Canvas 不算提交：提交后、只读回看时也能点 -->
        <UiButton
          v-if="action.openCanvas"
          size="small"
          :variant="action.style === 'primary' ? 'primary' : 'default'"
          :disabled="!sessionId"
          @click="click(action)"
        >{{ action.label }}</UiButton>
        <UiButton
          v-else-if="!submitted"
          size="small"
          :variant="action.style === 'primary' ? 'primary' : 'default'"
          :disabled="disabled"
          :tip="disabled ? disabledReason : undefined"
          @click="click(action)"
        >{{ action.label }}</UiButton>
      </template>
    </div>
  </div>
</template>
