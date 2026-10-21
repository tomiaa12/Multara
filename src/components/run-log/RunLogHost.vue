<script setup lang="ts">
import { computed } from "vue";
import RunLogDialog from "./RunLogDialog.vue";
import { closeRunLog, runLogPayload } from "./run-log-state";

const payload = computed(() => runLogPayload.value);
</script>

<template>
  <RunLogDialog v-if="payload" :payload="payload" @close="closeRunLog" />
</template>
