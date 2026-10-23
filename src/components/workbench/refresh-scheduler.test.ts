// @vitest-environment node

import { afterEach, describe, expect, test, vi } from "vitest";
import { createRefreshScheduler } from "./refresh-scheduler";

afterEach(() => {
  vi.useRealTimers();
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
});
