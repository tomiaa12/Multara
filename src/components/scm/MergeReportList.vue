<script setup lang="ts">
import { Check } from "@lucide/vue";
import type { MergeReport } from "./merge-report";

defineProps<{ report: MergeReport }>();
</script>

<template>
  <div class="merge-report">
    <ul class="merge-report-steps">
      <li v-for="(step, index) in report.steps" :key="index" :class="{ 'is-done': step.done }">
        <span class="merge-report-check" aria-hidden="true">
          <Check v-if="step.done" :size="11" :stroke-width="3" />
        </span>
        <span class="merge-report-label">{{ step.label }}</span>
      </li>
    </ul>
    <p v-for="(note, index) in report.notes" :key="`note:${index}`" class="merge-note is-warn">{{ note }}</p>
  </div>
</template>
