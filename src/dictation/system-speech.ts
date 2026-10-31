/** Copyright-shaped port of VS Code chat dictation capture → system SpeechRecognition. */
import { createSegmentAccumulator } from "./segment-accumulator";

export type SystemSpeechTranscript = {
  text: string;
  finalizedText: string;
  isFinal: boolean;
};

export type SystemSpeechErrorCode =
  | "not-allowed"
  | "no-speech"
  | "audio-capture"
  | "network"
  | "aborted"
  | "unsupported"
  | "unknown";

type SpeechRecognitionLike = {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  maxAlternatives: number;
  start: () => void;
  stop: () => void;
  abort: () => void;
  onresult: ((event: SpeechRecognitionEventLike) => void) | null;
  onerror: ((event: { error?: string }) => void) | null;
  onend: (() => void) | null;
};

type SpeechRecognitionEventLike = {
  resultIndex: number;
  results: ArrayLike<{
    isFinal: boolean;
    0?: { transcript?: string };
  } & { length: number }>;
};

type SpeechRecognitionCtor = new () => SpeechRecognitionLike;

function speechRecognitionCtor(): SpeechRecognitionCtor | null {
  const w = window as Window & {
    SpeechRecognition?: SpeechRecognitionCtor;
    webkitSpeechRecognition?: SpeechRecognitionCtor;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null;
}

/**
 * 环境是否允许使用系统听写（由后端判断）。Tauri 里先假定不允许，等后端回答后再放开，
 * 避免在答复回来之前用户点到麦克风而触发 macOS 的权限崩溃。
 */
let environmentAllowed = !("__TAURI_INTERNALS__" in window);

export function setSystemSpeechEnvironmentAllowed(allowed: boolean) {
  environmentAllowed = allowed;
}

export function isSystemSpeechSupported(): boolean {
  return environmentAllowed && Boolean(speechRecognitionCtor());
}

export type SystemSpeechSession = {
  stop: () => Promise<SystemSpeechTranscript>;
  cancel: () => void;
};

export type StartSystemSpeechOptions = {
  lang: string;
  onTranscript: (value: SystemSpeechTranscript) => void;
  onError: (code: SystemSpeechErrorCode, message: string) => void;
};

function mapError(code: string | undefined): SystemSpeechErrorCode {
  if (code === "not-allowed" || code === "service-not-allowed") return "not-allowed";
  if (code === "no-speech") return "no-speech";
  if (code === "audio-capture") return "audio-capture";
  if (code === "network") return "network";
  if (code === "aborted") return "aborted";
  return "unknown";
}

/**
 * Starts continuous system dictation. Some engines end the session after a
 * pause; while `active` we restart so the mic stays open until stop/cancel.
 */
export function startSystemSpeech(options: StartSystemSpeechOptions): SystemSpeechSession {
  const Ctor = speechRecognitionCtor();
  if (!Ctor) {
    options.onError("unsupported", "SpeechRecognition unavailable");
    return {
      stop: async () => ({ text: "", finalizedText: "", isFinal: true }),
      cancel: () => undefined,
    };
  }

  const recognition = new Ctor();
  recognition.continuous = true;
  recognition.interimResults = true;
  recognition.maxAlternatives = 1;
  recognition.lang = options.lang === "auto" ? "" : options.lang;

  const accumulator = createSegmentAccumulator(options.lang === "auto" ? "" : options.lang);
  let active = true;
  let last: SystemSpeechTranscript = { text: "", finalizedText: "", isFinal: false };
  let stopResolve: ((value: SystemSpeechTranscript) => void) | null = null;

  const emit = (isFinal: boolean) => {
    last = { ...last, isFinal };
    options.onTranscript(last);
  };

  recognition.onresult = (event) => {
    const next = accumulator.read(event.results);
    last = { text: next.text, finalizedText: next.finalizedText, isFinal: false };
    options.onTranscript(last);
  };

  recognition.onerror = (event) => {
    const code = mapError(event.error);
    if (code === "aborted" || code === "no-speech") return;
    active = false;
    options.onError(code, event.error ?? "speech error");
  };

  recognition.onend = () => {
    if (active) {
      // 引擎在停顿后自行结束：下一轮结果从头开始，所以先把已有文字固定下来。
      accumulator.commit(last.text);
      last = { text: last.text, finalizedText: last.text, isFinal: false };
      try {
        recognition.start();
      } catch {
        active = false;
        const finalResult = { ...last, isFinal: true };
        last = finalResult;
        stopResolve?.(finalResult);
        stopResolve = null;
      }
      return;
    }
    const finalResult = { ...last, isFinal: true };
    last = finalResult;
    emit(true);
    stopResolve?.(finalResult);
    stopResolve = null;
  };

  try {
    recognition.start();
  } catch (error) {
    active = false;
    options.onError(
      "unknown",
      error instanceof Error ? error.message : String(error),
    );
  }

  return {
    stop: () =>
      new Promise((resolve) => {
        if (!active) {
          resolve({ ...last, isFinal: true });
          return;
        }
        active = false;
        stopResolve = resolve;
        try {
          recognition.stop();
        } catch {
          resolve({ ...last, isFinal: true });
          stopResolve = null;
        }
        window.setTimeout(() => {
          if (!stopResolve) return;
          const finalResult = { ...last, isFinal: true };
          stopResolve(finalResult);
          stopResolve = null;
        }, 1500);
      }),
    cancel: () => {
      active = false;
      stopResolve = null;
      try {
        recognition.abort();
      } catch {
        /* ignore */
      }
    },
  };
}
