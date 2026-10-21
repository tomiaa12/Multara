<script setup lang="ts">
import { ref } from "vue";

const model = defineModel<string>();
const el = ref<HTMLTextAreaElement | null>(null);

function focus() {
  el.value?.focus();
}

defineExpose({ el, focus });
</script>

<template>
  <textarea ref="el" v-model="model" class="ui-textarea" />
</template>
