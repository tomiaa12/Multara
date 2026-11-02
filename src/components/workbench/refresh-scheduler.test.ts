// @vitest-environment node

import { afterEach, describe, expect, test, vi } from "vitest";
import { createRefreshScheduler } from "./refresh-scheduler";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("refresh scheduler", () => {
  test("debounces bursts and keeps at most one trailing refresh during cooldown", async () => {
    vi.useFakeTimers();
    let finish: (() => void) | undefined;
    const refresh = vi.fn(() => new Promise<void>((resolve) => {
      finish = resolve;
    }));
    const scheduler = createRefreshScheduler(refresh);

    scheduler.request();
    scheduler.request();
    scheduler.request();
    await vi.advanceTimersByTimeAsync(999);
    expect(refresh).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(refresh).toHaveBeenCalledTimes(1);

    scheduler.request();
    scheduler.request();
    finish?.();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(4_999);
    expect(refresh).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(refresh).toHaveBeenCalledTimes(2);

    scheduler.dispose();
  });

  test("dispose cancels a pending refresh", async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => undefined);
    const scheduler = createRefreshScheduler(refresh);
    scheduler.request();
    scheduler.dispose();
    await vi.runAllTimersAsync();
    expect(refresh).not.toHaveBeenCalled();
  });

  test("defers refresh until the window is focused again", async () => {
    vi.useFakeTimers();
    let focused = false;
    let wake: (() => void) | undefined;

    vi.stubGlobal("window", {
      addEventListener: (type: string, handler: () => void) => {
        if (type === "focus") wake = handler;
      },
      removeEventListener: () => undefined,
    });
    vi.stubGlobal("document", {
      visibilityState: "visible",
      hasFocus: () => focused,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    });

    const refresh = vi.fn(async () => undefined);
    const scheduler = createRefreshScheduler(refresh, {
      debounceMs: 100,
      cooldownMs: 5_000,
      isFocused: () => focused,
    });

    scheduler.request();
    await vi.advanceTimersByTimeAsync(100);
    await Promise.resolve();
    expect(refresh).not.toHaveBeenCalled();

    focused = true;
    wake?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(refresh).toHaveBeenCalledTimes(1);

    scheduler.dispose();
  });

  test("dispose cancels a refresh waiting for focus", async () => {
    vi.useFakeTimers();
    let focused = false;

    vi.stubGlobal("window", {
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    });
    vi.stubGlobal("document", {
      visibilityState: "hidden",
      hasFocus: () => focused,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    });

    const refresh = vi.fn(async () => undefined);
    const scheduler = createRefreshScheduler(refresh, {
      debounceMs: 100,
      isFocused: () => focused,
    });

    scheduler.request();
    await vi.advanceTimersByTimeAsync(100);
    await Promise.resolve();
    scheduler.dispose();
    focused = true;
    await Promise.resolve();
    expect(refresh).not.toHaveBeenCalled();
  });
});
