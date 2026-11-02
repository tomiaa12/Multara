export interface RefreshScheduler {
  request(): void;
  dispose(): void;
}

export interface RefreshSchedulerOptions {
  debounceMs?: number;
  cooldownMs?: number;
  /** When false, refresh waits until focused again (VS Code whenIdleAndFocused). */
  isFocused?: () => boolean;
}

const DEFAULT_DEBOUNCE_MS = 1_000;
const DEFAULT_COOLDOWN_MS = 5_000;

function defaultIsFocused(): boolean {
  if (typeof document === "undefined") return true;
  return document.visibilityState === "visible" && document.hasFocus();
}

export function createRefreshScheduler(
  refresh: () => Promise<void>,
  debounceMsOrOptions: number | RefreshSchedulerOptions = DEFAULT_DEBOUNCE_MS,
  cooldownMs = DEFAULT_COOLDOWN_MS,
): RefreshScheduler {
  const options: RefreshSchedulerOptions =
    typeof debounceMsOrOptions === "number"
      ? { debounceMs: debounceMsOrOptions, cooldownMs }
      : debounceMsOrOptions;
  const debounceMs = options.debounceMs ?? DEFAULT_DEBOUNCE_MS;
  const cooldown = options.cooldownMs ?? DEFAULT_COOLDOWN_MS;
  const isFocused = options.isFocused ?? defaultIsFocused;

  let timer: ReturnType<typeof setTimeout> | undefined;
  let running = false;
  let queued = false;
  let disposed = false;
  let nextAllowedAt = 0;
  let wakeFocusWait: (() => void) | undefined;

  function schedule(delay: number) {
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      void run();
    }, delay);
  }

  function waitUntilFocused(): Promise<void> {
    if (isFocused() || disposed) return Promise.resolve();
    return new Promise((resolve) => {
      const settle = () => {
        if (!disposed && !isFocused()) return;
        if (typeof window !== "undefined") window.removeEventListener("focus", settle);
        if (typeof document !== "undefined") document.removeEventListener("visibilitychange", settle);
        wakeFocusWait = undefined;
        resolve();
      };
      wakeFocusWait = settle;
      if (typeof window !== "undefined") window.addEventListener("focus", settle);
      if (typeof document !== "undefined") document.addEventListener("visibilitychange", settle);
    });
  }

  async function run() {
    if (disposed || running || !queued) return;
    const cooldownRemaining = nextAllowedAt - Date.now();
    if (cooldownRemaining > 0) {
      schedule(cooldownRemaining);
      return;
    }
    running = true;
    try {
      if (!isFocused()) {
        await waitUntilFocused();
        if (disposed || !queued) return;
      }
      queued = false;
      await refresh();
    } finally {
      running = false;
      nextAllowedAt = Date.now() + cooldown;
      if (queued && !disposed) schedule(cooldown);
    }
  }

  return {
    request() {
      if (disposed) return;
      queued = true;
      if (running) return;
      schedule(Math.max(debounceMs, nextAllowedAt - Date.now()));
    },
    dispose() {
      disposed = true;
      queued = false;
      clearTimeout(timer);
      wakeFocusWait?.();
    },
  };
}
