import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { dictationService, type DictationPhase } from "../dictation/dictation-service";
import { isSystemSpeechSupported, setSystemSpeechEnvironmentAllowed } from "../dictation/system-speech";
import * as backend from "../services/backend";
import { useAppStore } from "./app";

const ONBOARDING_KEY = "multara.dictation.onboarded";

export const useDictationStore = defineStore("dictation", () => {
  const phase = ref<DictationPhase>("idle");
  const activeTargetId = ref<string | null>(null);
  const lastError = ref<string | null>(null);
  const onboarded = ref(localStorage.getItem(ONBOARDING_KEY) === "1");
  const showOnboarding = ref(false);
  const supported = ref(isSystemSpeechSupported());
  /** 后端判定不可用的原因代码（如 devBuild），用于设置页说明。 */
  const unsupportedReason = ref<string | null>(null);

  void backend.dictationEnvironment().then((environment) => {
    setSystemSpeechEnvironmentAllowed(environment.supported);
    unsupportedReason.value = environment.supported ? null : environment.reason;
    supported.value = isSystemSpeechSupported();
  });

  dictationService.subscribe(() => {
    phase.value = dictationService.getPhase();
    activeTargetId.value = dictationService.getActiveTargetId();
    lastError.value = dictationService.getLastError();
  });

  const app = useAppStore();

  const enabled = computed(() => app.data?.settings.dictationEnabled !== "false");
  const language = computed(() => app.data?.settings.dictationLanguage ?? "auto");

  const visible = computed(() => supported.value && enabled.value);

  function markOnboarded() {
    onboarded.value = true;
    localStorage.setItem(ONBOARDING_KEY, "1");
    showOnboarding.value = false;
  }

  function openOnboarding() {
    showOnboarding.value = true;
  }

  async function setEnabled(value: boolean) {
    await app.saveSettings({ dictationEnabled: String(value) });
  }

  async function setLanguage(value: string) {
    await app.saveSettings({ dictationLanguage: value });
  }

  return {
    phase,
    activeTargetId,
    lastError,
    onboarded,
    showOnboarding,
    supported,
    unsupportedReason,
    enabled,
    language,
    visible,
    markOnboarded,
    openOnboarding,
    setEnabled,
    setLanguage,
  };
});
