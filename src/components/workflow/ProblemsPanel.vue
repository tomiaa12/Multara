<script setup lang="ts">
import { computed } from "vue";
import { AlertTriangle, ChevronDown, ChevronUp, CircleAlert } from "@lucide/vue";
import { t } from "../../i18n";
import type { Diagnostic } from "../../domain/workflow";

const props = defineProps<{ diagnostics: Diagnostic[]; open: boolean }>();
const emit = defineEmits<{ toggle: []; pick: [diagnostic: Diagnostic] }>();

const errors = computed(() => props.diagnostics.filter((item) => item.severity === "error").length);
const warnings = computed(() => props.diagnostics.length - errors.value);
const sorted = computed(() => [...props.diagnostics].sort((a, b) => Number(b.severity === "error") - Number(a.severity === "error")));
</script>

<template>
  <section class="problems" :class="{ open }">
    <button type="button" class="problems-head" @click="emit('toggle')">
      <component :is="open ? ChevronDown : ChevronUp" :size="13" />
      <strong>{{ t("workflow.problems.title") }}</strong>
      <span class="count error"><CircleAlert :size="12" />{{ errors }}</span>
      <span class="count warning"><AlertTriangle :size="12" />{{ warnings }}</span>
    </button>
    <div v-if="open" class="problems-body vs-scroll">
      <div v-if="!sorted.length" class="problems-empty">{{ t("workflow.problems.none") }}</div>
      <button v-for="(item, index) in sorted" :key="`${item.code}-${item.nodeId}-${item.edgeId}-${index}`" type="button" class="problem" @click="emit('pick', item)">
        <CircleAlert v-if="item.severity === 'error'" :size="13" class="error" />
        <AlertTriangle v-else :size="13" class="warning" />
        <span class="msg">{{ item.message }}</span>
        <code>{{ item.nodeId || item.edgeId || item.code }}</code>
        <small>{{ item.code }}</small>
      </button>
    </div>
  </section>
</template>

<style scoped>
.problems { flex: none; border-top: 1px solid var(--color-border); background: var(--color-surface); }
.problems-head { display: flex; align-items: center; gap: var(--spacing-2); width: 100%; height: 28px; padding: 0 var(--spacing-3); border: 0; background: transparent; color: var(--color-text-secondary); font-size: var(--font-size-sm); cursor: pointer; }
.problems-head:hover { background: var(--color-surface-hover); }
.count { display: inline-flex; align-items: center; gap: 3px; font-size: var(--font-size-xs); }
.error { color: var(--color-error); }
.warning { color: var(--color-warning); }
.problems-body { max-height: 160px; overflow-y: auto; border-top: 1px solid var(--color-border-light); }
.problems-empty { padding: var(--spacing-3); font-size: var(--font-size-sm); color: var(--color-text-tertiary); }
.problem { display: flex; align-items: center; gap: var(--spacing-2); width: 100%; min-height: 26px; padding: 0 var(--spacing-3); border: 0; background: transparent; text-align: left; color: var(--color-text); font-size: var(--font-size-sm); cursor: pointer; }
.problem:hover { background: var(--color-surface-hover); }
.problem .msg { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.problem code { font-family: var(--font-family-mono); font-size: var(--font-size-xs); color: var(--color-text-secondary); }
.problem small { color: var(--color-text-tertiary); font-size: var(--font-size-xs); }
</style>
