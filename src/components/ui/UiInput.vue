<script setup lang="ts">
withDefaults(defineProps<{
  type?: string;
}>(), {
  type: "text",
});

const [model, modifiers] = defineModel<unknown>({
  set(value) {
    if (modifiers.number && typeof value === "string" && value !== "") {
      const number = Number(value);
      return Number.isNaN(number) ? value : number;
    }
    return modifiers.trim && typeof value === "string" ? value.trim() : value;
  },
});
</script>

<template>
  <input
    v-model="model"
    class="ui-input"
    :class="{ 'ui-checkbox': type === 'checkbox' || type === 'radio' }"
    :type="type"
  />
</template>
