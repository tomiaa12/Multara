<script setup lang="ts">
import { computed, ref } from "vue";
import { X } from "@lucide/vue";
import { t } from "../../i18n";

export type OverflowSummaryItem = {
  id: string;
  label: string;
  color?: string;
  detail?: string;
  badge?: string;
};

const props = withDefaults(
  defineProps<{
    items: OverflowSummaryItem[];
    /** 直接展示的条数，超出后跟「等 n 个单位」 */
    preview?: number;
    /** 只显示「n 个单位」，不展示条目 chip */
    countOnly?: boolean;
    unit: string;
    dialogTitle: string;
    emptyText?: string;
  }>(),
  {
    preview: 1,
    countOnly: false,
    emptyText: "",
  },
);

const showAll = ref(false);

const shown = computed(() =>
  props.countOnly ? [] : props.items.slice(0, Math.max(0, props.preview)),
);
const rest = computed(() => Math.max(0, props.items.length - shown.value.length));
const empty = computed(() => props.emptyText || t("overflowSummary.empty"));
const countLabel = computed(() =>
  t("overflowSummary.count", { n: props.items.length, unit: props.unit }),
);

function openAll(event: MouseEvent) {
  event.stopPropagation();
  showAll.value = true;
}
</script>

<template>
  <div class="overflow-summary" @click.stop>
    <template v-if="items.length">
      <template v-if="countOnly">
        <button type="button" class="overflow-summary-more" @click="openAll">
          {{ countLabel }}
        </button>
      </template>
      <template v-else>
        <span v-for="item in shown" :key="item.id" class="overflow-summary-chip">
          <i v-if="item.color" class="tag-dot" :style="{ background: item.color }" />
          <span>{{ item.label }}</span>
        </span>
        <button
          v-if="rest"
          type="button"
          class="overflow-summary-more"
          @click="openAll"
        >
          {{ t("overflowSummary.more", { n: rest, unit }) }}
        </button>
        <button
          v-else-if="items.length > 1"
          type="button"
          class="overflow-summary-more is-quiet"
          @click="openAll"
        >
          {{ t("overflowSummary.viewAll") }}
        </button>
      </template>
    </template>
    <span v-else class="overflow-summary-empty">{{ empty }}</span>
  </div>

  <UiModal v-if="showAll" @click.self="showAll = false">
    <div class="modal overflow-summary-dialog" role="dialog" aria-modal="true">
      <header class="modal-heading">
        <h2>{{ dialogTitle }}</h2>
        <UiButton class="icon-button" size="icon" :tip="t('common.close')" @click="showAll = false">
          <X :size="15" />
        </UiButton>
      </header>
      <div class="modal-body vs-scroll">
        <ul class="overflow-summary-list">
          <li v-for="item in items" :key="item.id">
            <i v-if="item.color" class="tag-dot" :style="{ background: item.color }" />
            <div class="overflow-summary-meta">
              <strong>{{ item.label }}</strong>
              <small v-if="item.detail">{{ item.detail }}</small>
            </div>
            <span v-if="item.badge" class="ds-badge">{{ item.badge }}</span>
          </li>
        </ul>
      </div>
      <div class="modal-actions">
        <UiButton @click="showAll = false">{{ t("common.close") }}</UiButton>
      </div>
    </div>
  </UiModal>
</template>
