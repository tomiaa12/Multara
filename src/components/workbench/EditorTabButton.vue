<script setup lang="ts">
import { X } from "@lucide/vue";
import { t } from "../../i18n";
import UiTooltip from "../ui/UiTooltip.vue";

withDefaults(defineProps<{
  tip: string;
  name: string;
  active?: boolean;
  preview?: boolean;
  dirty?: boolean;
  diffShort?: string;
  closeTip?: string;
}>(), {
  active: false,
  preview: false,
  dirty: false,
  diffShort: "",
  closeTip: "",
});

const emit = defineEmits<{
  select: [];
  pin: [];
  close: [event: MouseEvent];
  contextmenu: [event: MouseEvent];
}>();

function onClose(event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  emit("close", event);
}

function onMiddle(event: MouseEvent) {
  event.preventDefault();
  emit("close", event);
}
</script>

<template>
  <UiTooltip :content="tip">
    <button
      type="button"
      class="editor-tab"
      :class="{
        active,
        'is-preview': preview,
        'is-dirty': dirty,
      }"
      @click="emit('select')"
      @dblclick="emit('pin')"
      @click.middle.prevent="onMiddle"
      @mousedown.middle.prevent
      @mousedown="$event.button === 2 && $event.preventDefault()"
      @contextmenu="emit('contextmenu', $event)"
    >
      <slot />
      <span class="editor-tab-name">{{ name }}</span>
      <span v-if="diffShort" class="editor-tab-diff">{{ diffShort }}</span>
      <UiTooltip :content="closeTip || t('common.close')">
        <span class="tab-close" :class="{ 'is-dirty': dirty }" @click="onClose">
          <X :size="12" />
        </span>
      </UiTooltip>
    </button>
  </UiTooltip>
</template>
