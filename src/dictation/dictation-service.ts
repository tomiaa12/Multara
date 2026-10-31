import {
  createComposerInserter,
  DICTATION_MAX_SESSION_MS,
  type ComposerInserter,
  type ComposerInserterHost,
} from "./composer-inserter";
import { resolveDictationLanguage } from "./dictation-language";
import {
  isSystemSpeechSupported,
  startSystemSpeech,
  type SystemSpeechErrorCode,
  type SystemSpeechSession,
} from "./system-speech";
import { createTranscriptReducer } from "./transcript-reducer";

export type DictationPhase = "idle" | "preparing" | "recording" | "transcribing";

export type DictationTarget = ComposerInserterHost & {
  id: string;
};

type Listener = () => void;

const listeners = new Set<Listener>();

let phase: DictationPhase = "idle";
let activeTargetId: string | null = null;
let lastError: string | null = null;
let session: SystemSpeechSession | null = null;
let inserter: ComposerInserter | null = null;
let revision = 0;
let maxTimer = 0;
let language = "";
let stopPromise: Promise<void> | null = null;

const reducer = createTranscriptReducer();

function onDocumentKey(event: KeyboardEvent) {
  if (event.key !== "Escape" || phase === "idle") return;
  event.preventDefault();
  void endSession("stop");
}

function onVisibility() {
  if (document.visibilityState === "hidden" && phase !== "idle") {
    void endSession("stop");
  }
}

let globalHooks = false;
function ensureGlobalHooks() {
  if (globalHooks) return;
  globalHooks = true;
  window.addEventListener("keydown", onDocumentKey, true);
  document.addEventListener("visibilitychange", onVisibility);
}

function emit() {
  for (const listener of listeners) listener();
}

function setPhase(next: DictationPhase) {
  phase = next;
  emit();
}

function clearMaxTimer() {
  if (!maxTimer) return;
  window.clearTimeout(maxTimer);
  maxTimer = 0;
}

function teardownInserter(finalize: boolean) {
  if (!inserter) return;
  if (finalize) inserter.finish();
  else inserter.dispose();
  inserter = null;
}

async function endSession(mode: "stop" | "cancel") {
  clearMaxTimer();
  const current = session;
  session = null;
  if (!current) {
    teardownInserter(mode === "stop");
    activeTargetId = null;
    setPhase("idle");
    return;
  }
  if (mode === "cancel") {
    current.cancel();
    teardownInserter(false);
    activeTargetId = null;
    setPhase("idle");
    return;
  }
  setPhase("transcribing");
  try {
    const finalResult = await Promise.race([
      current.stop(),
      new Promise<null>((resolve) => window.setTimeout(() => resolve(null), 1500)),
    ]);
    if (finalResult) {
      revision += 1;
      const applied = reducer.apply({
        revision,
        text: finalResult.text,
        finalizedText: finalResult.finalizedText,
        isFinal: true,
      });
      if (applied) inserter?.update(applied.text, applied.finalizedText);
    }
  } finally {
    teardownInserter(true);
    activeTargetId = null;
    setPhase("idle");
  }
}

export const dictationService = {
  isSupported: isSystemSpeechSupported,

  subscribe(listener: Listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },

  getPhase: () => phase,
  getActiveTargetId: () => activeTargetId,
  getLastError: () => lastError,
  isActive: () => phase !== "idle",
  isActiveFor: (id: string) => activeTargetId === id && phase !== "idle",

  async start(target: DictationTarget, options: {
    configuredLanguage?: string;
    displayLanguage?: string;
  }) {
    if (!isSystemSpeechSupported()) {
      lastError = "unsupported";
      emit();
      return;
    }
    if (activeTargetId && activeTargetId !== target.id) {
      await endSession("stop");
    }
    if (phase !== "idle") {
      await endSession("stop");
      return;
    }

    ensureGlobalHooks();
    lastError = null;
    reducer.reset();
    revision = 0;
    activeTargetId = target.id;
    language = resolveDictationLanguage(options.configuredLanguage, options.displayLanguage);
    setPhase("preparing");

    inserter = createComposerInserter(target, { language: language || "auto" });

    session = startSystemSpeech({
      lang: language || "auto",
      onTranscript: (value) => {
        revision += 1;
        const applied = reducer.apply({
          revision,
          text: value.text,
          finalizedText: value.finalizedText,
          isFinal: value.isFinal,
        });
        if (!applied) return;
        inserter?.update(applied.text, applied.finalizedText);
      },
      onError: (code: SystemSpeechErrorCode) => {
        if (code === "aborted") return;
        lastError = code;
        void endSession("cancel");
      },
    });

    setPhase("recording");
    clearMaxTimer();
    maxTimer = window.setTimeout(() => {
      void dictationService.stop();
    }, DICTATION_MAX_SESSION_MS);
  },

  async stop() {
    if (stopPromise) return stopPromise;
    stopPromise = endSession("stop").finally(() => {
      stopPromise = null;
    });
    return stopPromise;
  },

  async cancel() {
    await endSession("cancel");
  },

  async toggle(target: DictationTarget, options: {
    configuredLanguage?: string;
    displayLanguage?: string;
  }) {
    if (this.isActiveFor(target.id)) {
      await this.stop();
      return;
    }
    await this.start(target, options);
  },

  /** Before send: wait for final transcript (with timeout). */
  async stopBeforeSubmit(targetId: string) {
    if (!this.isActiveFor(targetId)) return;
    await this.stop();
  },
};
