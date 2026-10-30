<script setup lang="ts">
import { useAttrs } from "vue";
import UiTooltip from "./UiTooltip.vue";
import type { TooltipPlacement } from "./tooltip-placement";

defineOptions({ inheritAttrs: false });

withDefaults(defineProps<{
  type?: "button" | "submit" | "reset";
  variant?: "default" | "primary" | "danger" | "danger-ghost" | "ghost";
  size?: "default" | "small" | "icon";
  /** 悬停提示，走公共 UiTooltip（不要再用原生 title） */
  tip?: string;
  tipPlacement?: TooltipPlacement;
}>(), {
  type: "button",
  variant: "default",
  size: "default",
  tip: "",
  tipPlacement: "top",
});

const attrs = useAttrs();
</script>

<template>
  <UiTooltip v-if="tip" :content="tip" :placement="tipPlacement">
    <button
      class="ui-button"
      :type="type"
      :data-variant="variant"
      :data-size="size"
      v-bind="attrs"
    >
      <slot />
    </button>
  </UiTooltip>
  <button
    v-else
    class="ui-button"
    :type="type"
    :data-variant="variant"
    :data-size="size"
    v-bind="attrs"
  >
    <slot />
  </button>
</template>
