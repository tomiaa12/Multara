<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { ArrowLeft, ArrowRight, ExternalLink, RotateCw } from "@lucide/vue";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import Codicon from "../../components/ui/Codicon.vue";
import UiButton from "../../components/ui/UiButton.vue";
import { normalizeBrowserUrl } from "./inbox-browser-links";

const props = defineProps<{
  links: string[];
  openUrl?: string;
}>();

const stack = ref<string[]>([]);
const index = ref(-1);
const draft = ref("");
const reloadNonce = ref(0);
const picker = ref<"" | "history" | "address">("");
const pickerQuery = ref("");
const pickerIndex = ref(0);
const pickerInput = ref<HTMLInputElement | null>(null);

const current = computed(() => stack.value[index.value] ?? "");
const canBack = computed(() => index.value > 0);
const canForward = computed(() => index.value >= 0 && index.value < stack.value.length - 1);
const historyMatches = computed(() => {
  const query = pickerQuery.value.trim().toLowerCase();
  if (!query) return props.links;
  return props.links.filter((url) => url.toLowerCase().includes(query));
});
const pickerPlaceholder = computed(() => {
  if (picker.value === "history") return t("inbox.tool.historyPlaceholder");
  return t("inbox.tool.address");
});

function navigate(raw: string) {
  const url = normalizeBrowserUrl(raw);
  if (!url) return;
  stack.value = [...stack.value.slice(0, index.value + 1), url];
  index.value = stack.value.length - 1;
  draft.value = url;
}

function back() {
  if (!canBack.value) return;
  index.value -= 1;
  draft.value = current.value;
}

function forward() {
  if (!canForward.value) return;
  index.value += 1;
  draft.value = current.value;
}

function reload() {
  if (!current.value) return;
  reloadNonce.value += 1;
}

function onSubmit() {
  navigate(draft.value);
}

function openPicker(kind: "history" | "address") {
  picker.value = kind;
  pickerQuery.value = "";
  pickerIndex.value = 0;
  void nextTick(() => pickerInput.value?.focus());
}

function closePicker() {
  picker.value = "";
}

function movePicker(delta: number) {
  const count = picker.value === "history" ? historyMatches.value.length : 0;
  if (!count) return;
  pickerIndex.value = (pickerIndex.value + delta + count) % count;
}

function confirmPicker() {
  if (picker.value === "history") {
    const url = historyMatches.value[pickerIndex.value];
    if (url) navigate(url);
    closePicker();
    return;
  }
  navigate(pickerQuery.value);
  closePicker();
}

watch(historyMatches, () => {
  pickerIndex.value = 0;
});

watch(() => props.openUrl, (url) => {
  if (url) navigate(url);
});
</script>

<template>
  <section class="inbox-browser">
    <form class="inbox-browser-bar" @submit.prevent="onSubmit">
      <UiButton variant="ghost" size="icon" :tip="t('inbox.tool.back')" :disabled="!canBack" @click="back">
        <ArrowLeft :size="14" />
      </UiButton>
      <UiButton variant="ghost" size="icon" :tip="t('inbox.tool.forward')" :disabled="!canForward" @click="forward">
        <ArrowRight :size="14" />
      </UiButton>
      <UiButton variant="ghost" size="icon" :tip="t('inbox.tool.reload')" :disabled="!current" @click="reload">
        <RotateCw :size="14" />
      </UiButton>
      <input
        v-model="draft"
        class="inbox-browser-address"
        type="text"
        spellcheck="false"
        :placeholder="t('inbox.tool.address')"
        :aria-label="t('inbox.tool.address')"
      />
      <UiButton
        variant="ghost"
        size="icon"
        :tip="t('inbox.tool.openExternal')"
        :disabled="!current"
        @click="backend.openExternalUrl(current)"
      >
        <ExternalLink :size="14" />
      </UiButton>
    </form>
    <iframe
      v-if="current"
      :key="`${current}\n${reloadNonce}`"
      class="inbox-browser-frame"
      :src="current"
      :title="current"
    />
    <div v-else class="inbox-browser-empty">
      <div class="inbox-browser-actions sessions-watermark-actions" role="toolbar" :aria-label="t('inbox.tool.browser')">
        <button type="button" class="sessions-add-tab-watermark-entry" @click="openPicker('history')">
          <Codicon name="history" class="sessions-add-tab-watermark-icon" :size="16" />
          <span class="sessions-add-tab-watermark-label">{{ t("inbox.tool.history") }}</span>
        </button>
        <button type="button" class="sessions-add-tab-watermark-entry" @click="openPicker('address')">
          <Codicon name="link" class="sessions-add-tab-watermark-icon" :size="16" />
          <span class="sessions-add-tab-watermark-label">{{ t("inbox.tool.openAddress") }}</span>
        </button>
      </div>
    </div>
    <div
      v-if="picker"
      class="quick-pick"
      role="dialog"
      :aria-label="pickerPlaceholder"
    >
      <input
        ref="pickerInput"
        v-model="pickerQuery"
        :placeholder="pickerPlaceholder"
        :aria-label="pickerPlaceholder"
        @keydown.escape.stop="closePicker"
        @keydown.enter.prevent="confirmPicker"
        @keydown.down.prevent="movePicker(1)"
        @keydown.up.prevent="movePicker(-1)"
      />
      <template v-if="picker === 'history'">
        <button
          v-for="(url, itemIndex) in historyMatches"
          :key="url"
          type="button"
          :class="{ active: itemIndex === pickerIndex }"
          @click="navigate(url); closePicker()"
          @mouseenter="pickerIndex = itemIndex"
        >
          <Codicon name="globe" :size="16" />
          <span>{{ url }}</span>
        </button>
        <p v-if="!historyMatches.length">{{ t("inbox.tool.noPages") }}</p>
      </template>
      <button
        v-else-if="pickerQuery.trim()"
        type="button"
        class="active"
        @click="confirmPicker"
      >
        <Codicon name="arrow-right" :size="16" />
        <span>{{ t("inbox.tool.goTo", { url: pickerQuery.trim() }) }}</span>
      </button>
    </div>
  </section>
</template>
