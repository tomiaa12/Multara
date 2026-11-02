<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, useAttrs } from "vue";
import { escapeClosesDialog } from "./dialog-escape";

/**
 * 全屏遮罩弹层：Teleport 到 body，避免落在 .vs-scroll（transform）内时
 * position:fixed 相对滚动容器定位，滚动后弹窗跑到页面顶部。
 * 默认 Esc 关闭（emit close，并触发已有的 click.self / mousedown.self）。
 */
defineOptions({ inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    closeOnBackdrop?: boolean;
    closeOnEsc?: boolean;
  }>(),
  {
    closeOnBackdrop: false,
    closeOnEsc: true,
  },
);

const emit = defineEmits<{
  close: [];
}>();

const attrs = useAttrs();
const root = ref<HTMLElement | null>(null);

const backdropAttrs = computed(() =>
  props.closeOnBackdrop
    ? attrs
    : Object.fromEntries(
        Object.entries(attrs).filter(
          ([key]) => key !== "onClick" && key !== "onMousedown",
        ),
      ),
);

function isTopModal() {
  const el = root.value;
  if (!el) return false;
  const stack = document.querySelectorAll(".modal-backdrop");
  return stack[stack.length - 1] === el;
}

function invokeSelfHandler(type: "onClick" | "onMousedown") {
  const handler = attrs[type];
  const el = root.value;
  if (typeof handler !== "function" || !el) return;
  const event = new MouseEvent(type === "onClick" ? "click" : "mousedown", {
    bubbles: false,
    cancelable: true,
    view: window,
  });
  Object.defineProperty(event, "target", { value: el });
  Object.defineProperty(event, "currentTarget", { value: el });
  handler(event);
}

function onKeydown(event: KeyboardEvent) {
  if (!props.closeOnEsc || !escapeClosesDialog(event) || !isTopModal()) return;
  event.preventDefault();
  // 已有 click.self / mousedown.self 时走原回调（可带 busy/blocked 判断）；否则 emit close
  const hasSelf =
    typeof attrs.onClick === "function" || typeof attrs.onMousedown === "function";
  if (hasSelf) {
    invokeSelfHandler("onClick");
    invokeSelfHandler("onMousedown");
    return;
  }
  emit("close");
}

onMounted(() => document.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => document.removeEventListener("keydown", onKeydown));
</script>

<template>
  <Teleport to="body">
    <div ref="root" class="modal-backdrop" v-bind="backdropAttrs">
      <slot />
    </div>
  </Teleport>
</template>
