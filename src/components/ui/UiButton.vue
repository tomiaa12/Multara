<script setup lang="ts">
withDefaults(defineProps<{
  type?: "button" | "submit" | "reset";
  variant?: "default" | "primary" | "danger" | "danger-ghost" | "ghost";
  size?: "default" | "small" | "icon";
}>(), {
  type: "button",
  variant: "default",
  size: "default",
});
</script>

<template>
  <button
    class="ui-button"
    :type="type"
    :data-variant="variant"
    :data-size="size"
  >
    <slot />
  </button>
</template>
