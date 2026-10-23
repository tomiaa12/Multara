export interface RefreshScheduler {
  request(): void;
  dispose(): void;
}

export function createRefreshScheduler(
  refresh: () => Promise<void>,
  debounceMs = 1_000,
  cooldownMs = 5_000,
): RefreshScheduler {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let running = false;
  let queued = false;
  let disposed = false;
  let nextAllowedAt = 0;

  function schedule(delay: number) {
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      void run();
    }, delay);
  }

  async function run() {
    if (disposed || running || !queued) return;
    const cooldownRemaining = nextAllowedAt - Date.now();
    if (cooldownRemaining > 0) {
      schedule(cooldownRemaining);
      return;
    }
    queued = false;
    running = true;
    try {
      await refresh();
    } finally {
      running = false;
      nextAllowedAt = Date.now() + cooldownMs;
      if (queued && !disposed) schedule(cooldownMs);
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
    },
  };
}
