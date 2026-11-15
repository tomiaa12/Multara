<script setup lang="ts">
import { computed } from "vue";
import { RefreshCw } from "@lucide/vue";
import { t } from "../i18n";
import type { RuntimeUsage } from "../services/backend";

/** 列表紧凑模式最多展示几条，与 min-height 对齐 */
const COMPACT_SLOT_COUNT = 2;

const props = withDefaults(
  defineProps<{
    usage?: RuntimeUsage | null;
    loading?: boolean;
    compact?: boolean;
    showRefresh?: boolean;
  }>(),
  { usage: null, loading: false, compact: false, showRefresh: false },
);

const emit = defineEmits<{ refresh: [] }>();

const windows = computed(() => props.usage?.windows ?? []);
const visibleWindows = computed(() =>
  props.compact ? windows.value.slice(0, COMPACT_SLOT_COUNT) : windows.value,
);
const remaining = (usedPercent: number) => Math.max(0, Math.round(100 - usedPercent));

function resetsLabel(resetsAt?: string) {
  if (!resetsAt) return "";
  const ms = Date.parse(resetsAt);
  if (!Number.isFinite(ms)) return "";
  const delta = ms - Date.now();
  if (delta <= 0) return t("runtimeUsage.resetSoon");
  const mins = Math.round(delta / 60_000);
  const HOUR_MINS = 60;
  const DAY_MINS = 24 * HOUR_MINS;
  if (mins < HOUR_MINS) return t("runtimeUsage.resetsInMins", { n: Math.max(1, mins) });
  if (mins < DAY_MINS) return t("runtimeUsage.resetsInHours", { n: Math.round(mins / HOUR_MINS) });
  return t("runtimeUsage.resetsInDays", { n: Math.round(mins / DAY_MINS) });
}

const hint = computed(() => {
  if (windows.value.length) return "";
  const unavailable = props.usage?.unavailable;
  if (!unavailable) return "";
  if (unavailable.message) return unavailable.message;
  if (unavailable.reason === "noCredentials") return t("runtimeUsage.noCredentials");
  if (unavailable.reason === "probeFailed") return t("runtimeUsage.probeFailed");
  return t("runtimeUsage.unsupported");
});

const meta = computed(() => {
  const parts = [props.usage?.plan, props.usage?.email].filter(Boolean);
  return parts.join(" · ");
});
</script>

<template>
  <div class="runtime-usage" :data-compact="compact || undefined">
    <header v-if="!compact" class="runtime-usage-head">
      <strong>{{ t("runtimeUsage.title") }}</strong>
      <span v-if="meta" class="runtime-usage-meta">{{ meta }}</span>
      <UiButton
        v-if="showRefresh"
        class="icon-button"
        size="icon"
        :disabled="loading"
        :tip="t('runtimeUsage.refresh')"
        @click="emit('refresh')"
      >
        <RefreshCw :size="14" :class="{ spin: loading }" />
      </UiButton>
    </header>
    <ul v-if="visibleWindows.length" class="runtime-usage-list">
      <li v-for="window in visibleWindows" :key="window.id">
        <div class="runtime-usage-row">
          <span>{{ window.label }}</span>
          <small>
            {{ t("runtimeUsage.left", { n: remaining(window.usedPercent) }) }}
            <template v-if="resetsLabel(window.resetsAt)"> · {{ resetsLabel(window.resetsAt) }}</template>
          </small>
        </div>
        <div
          class="ds-progress"
          role="img"
          :aria-label="`${window.label}: ${remaining(window.usedPercent)}%`"
        >
          <span :style="{ width: `${Math.min(100, Math.max(0, window.usedPercent))}%` }" />
        </div>
      </li>
    </ul>
    <p v-else-if="hint" class="runtime-usage-hint">{{ hint }}</p>
  </div>
</template>

<style scoped>
.runtime-usage {
  display: grid;
  gap: 8px;
  min-width: 0;
}
.runtime-usage[data-compact] {
  align-content: start;
}
.runtime-usage-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.runtime-usage-head strong {
  font-size: var(--font-size-sm);
}
.runtime-usage-meta {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--color-text-tertiary);
  font-size: var(--font-size-xs);
}
.runtime-usage-hint {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: var(--font-size-xs);
  line-height: 1.4;
}
.runtime-usage-list {
  display: grid;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.runtime-usage-row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: var(--font-size-xs);
}
.runtime-usage-row small {
  color: var(--color-text-tertiary);
  white-space: nowrap;
}
.runtime-usage[data-compact] .runtime-usage-list {
  gap: 4px;
}
.spin {
  animation: runtime-usage-spin 0.8s linear infinite;
}
@keyframes runtime-usage-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
