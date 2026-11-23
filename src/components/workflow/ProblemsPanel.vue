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
const lead = computed(() => sorted.value.find((item) => item.severity === "error") ?? sorted.value[0] ?? null);
const more = computed(() => Math.max(0, sorted.value.length - (lead.value ? 1 : 0)));
</script>

<template>
  <section class="problems" :class="{ open, alert: errors > 0 }">
    <button type="button" class="problems-head" :title="t('workflow.problems.openHint')" @click="emit('toggle')">
      <component :is="open ? ChevronDown : ChevronUp" :size="13" />
      <strong>{{ t("workflow.problems.title") }}</strong>
      <span class="count error"><CircleAlert :size="12" />{{ errors }}</span>
      <span class="count warning"><AlertTriangle :size="12" />{{ warnings }}</span>
      <span v-if="!open && lead" class="lead">{{ lead.message }}</span>
      <span v-if="!open && more" class="more">{{ t("workflow.problems.more", { count: more }) }}</span>
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
.problems.alert { border-top-color: var(--color-error-border); }
.problems-head { display: flex; align-items: center; justify-content: flex-start; gap: var(--spacing-2); width: 100%; min-height: 28px; padding: 0 var(--spacing-3); border: 0; background: transparent; color: var(--color-text-secondary); font-size: var(--font-size-sm); cursor: pointer; text-align: left; }
.problems.alert .problems-head { background: var(--color-error-subtle); color: var(--color-error); }
.problems-head:hover { background: var(--color-surface-hover); }
.problems.alert .problems-head:hover { background: var(--color-error-subtle); }
.lead { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--color-text); font-weight: var(--font-weight-regular); }
.more { flex: none; color: var(--color-text-tertiary); font-size: var(--font-size-xs); }
.count { display: inline-flex; align-items: center; gap: 3px; font-size: var(--font-size-xs); }
.error { color: var(--color-error); }
.warning { color: var(--color-warning); }
.problems-body { max-height: 160px; overflow-y: auto; border-top: 1px solid var(--color-border-light); }
.problems-empty { padding: var(--spacing-3); font-size: var(--font-size-sm); color: var(--color-text-tertiary); }
.problem { display: flex; align-items: center; justify-content: flex-start; gap: var(--spacing-2); width: 100%; min-height: 26px; padding: 0 var(--spacing-3); border: 0; background: transparent; text-align: left; color: var(--color-text); font-size: var(--font-size-sm); cursor: pointer; }
.problem:hover { background: var(--color-surface-hover); }
.problem .msg { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.problem code { font-family: var(--font-family-mono); font-size: var(--font-size-xs); color: var(--color-text-secondary); }
.problem small { color: var(--color-text-tertiary); font-size: var(--font-size-xs); }
</style>
