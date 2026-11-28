<script setup lang="ts">
import { computed } from "vue";
import SessionRawDataDialog from "./SessionRawDataDialog.vue";
import { closeSessionRawData, sessionRawPayload } from "./raw-data-state";

const payload = computed(() => sessionRawPayload.value);
</script>

<template>
  <SessionRawDataDialog
    v-if="payload"
    :session-id="payload.sessionId"
    :title="payload.title"
    @close="closeSessionRawData"
  />
</template>
