<script setup lang="ts">
import { computed, onBeforeUnmount, onDeactivated, onMounted, ref } from "vue";
import Codicon from "../ui/Codicon.vue";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu.vue";
import { dictationService } from "../../dictation/dictation-service";
import { useDictationStore } from "../../stores/dictation";
import { locale, t } from "../../i18n";
import DictationOnboardingCard from "./DictationOnboardingCard.vue";

const props = defineProps<{
  targetId: string;
  getEditor: () => HTMLElement | null;
  notifyChange: () => void;
  isComposing: () => boolean;
}>();

const store = useDictationStore();
const menu = ref<{ x: number; y: number } | null>(null);
const liveMessage = ref("");

const recording = computed(() => store.phase === "recording" && store.activeTargetId === props.targetId);
const busy = computed(
  () =>
    (store.phase === "preparing" || store.phase === "transcribing")
    && store.activeTargetId === props.targetId,
);
const pressed = computed(() => recording.value || busy.value);

const menuItems = computed<ContextMenuItem[]>(() => [
  {
    id: "toggle",
    label: recording.value ? t("dictation.stop") : t("dictation.start"),
  },
  { type: "sep" },
  { id: "onboarding", label: t("dictation.showIntro") },
  { id: "disable", label: t("dictation.disable") },
]);

function target() {
  return {
    id: props.targetId,
    getEditor: props.getEditor,
    notifyChange: props.notifyChange,
    isComposing: props.isComposing,
  };
}

async function toggle() {
  if (!store.onboarded) {
    store.openOnboarding();
    return;
  }
  liveMessage.value = recording.value ? t("dictation.stopped") : t("dictation.listening");
  await dictationService.toggle(target(), {
    configuredLanguage: store.language,
    displayLanguage: locale.value,
  });
}

function onContext(event: MouseEvent) {
  event.preventDefault();
  menu.value = { x: event.clientX, y: event.clientY };
}

async function onMenuSelect(id: string) {
  menu.value = null;
  if (id === "toggle") {
    await toggle();
    return;
  }
  if (id === "onboarding") {
    store.openOnboarding();
    return;
  }
  if (id === "disable") {
    await dictationService.stop();
    await store.setEnabled(false);
  }
}

async function onOnboarded() {
  store.markOnboarded();
  await toggle();
}

function onKey(event: KeyboardEvent) {
  const ctrl = event.ctrlKey || event.metaKey;
  if (!ctrl || !event.shiftKey || event.altKey || event.code !== "KeyM") return;
  const el = props.getEditor();
  if (!el) return;
  const active = document.activeElement;
  if (active !== el && !el.contains(active) && active?.closest?.(`[data-dictation-target="${props.targetId}"]`) == null) {
    // Allow when focus is inside this composer host.
    const host = el.closest(".chat-composer");
    if (!host?.contains(active)) return;
  }
  event.preventDefault();
  void toggle();
}

onMounted(() => window.addEventListener("keydown", onKey, true));
// keep-alive 页面被切走时组件不会卸载，听写要在这里停。
onDeactivated(() => {
  if (dictationService.isActiveFor(props.targetId)) void dictationService.stop();
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey, true);
  if (dictationService.isActiveFor(props.targetId)) void dictationService.stop();
});
</script>

<template>
  <div class="dictation-mic-wrap" :data-dictation-target="targetId">
    <span class="sr-only" aria-live="polite">{{ liveMessage }}</span>
    <UiButton
      type="button"
      class="icon-button dictation-mic"
      variant="ghost"
      :class="{ 'is-recording': recording, 'is-busy': busy }"
      :aria-pressed="pressed"
      :aria-label="recording ? t('dictation.stop') : t('dictation.start')"
      :tip="recording ? t('dictation.stop') : t('dictation.start')"
      @click="toggle"
      @contextmenu="onContext"
    >
      <Codicon v-if="busy" name="loading" :size="15" spin />
      <Codicon v-else :name="recording ? 'mic-filled' : 'mic'" :size="15" />
    </UiButton>
    <ContextMenu
      v-if="menu"
      :x="menu.x"
      :y="menu.y"
      :items="menuItems"
      @select="onMenuSelect"
      @close="menu = null"
    />
    <DictationOnboardingCard
      v-if="store.showOnboarding"
      @confirm="onOnboarded"
      @cancel="store.showOnboarding = false"
    />
  </div>
</template>
