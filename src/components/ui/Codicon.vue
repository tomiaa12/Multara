<script setup lang="ts">
import "../../assets/codicons/codicon.css";

withDefaults(defineProps<{
  name: string;
  /** Glyph size in px. VS Code draws these at 16, and tree twisties at 10. */
  size?: number;
  spin?: boolean;
}>(), {
  size: 16,
  spin: false,
});
</script>

<template>
  <span
    class="codicon"
    :class="[`codicon-${name}`, { 'codicon-modifier-spin': spin }]"
    :style="{ fontSize: `${size}px` }"
    aria-hidden="true"
  />
</template>
