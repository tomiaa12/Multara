// @vitest-environment node
import { describe, expect, it, vi } from "vitest";
import { attachTimelineWeights, loadCommitFrequencies } from "./scm-inspect";

vi.mock("../../services/backend", () => ({
  gitCommitFiles: vi.fn(async (_workspaceId: string, hash: string) => {
    await new Promise((resolve) => setTimeout(resolve, 5));
    return [{ path: `${hash}.ts`, status: "M" }];
  }),
  gitLog: vi.fn(async () => ({
    items: Array.from({ length: 12 }, (_, index) => ({
      hash: `h${index}`,
      subject: `c${index}`,
      author: "a",
      committedAt: new Date(Date.now() - index * 86_400_000).toISOString(),
      parents: [] as string[],
      refs: [] as string[],
    })),
    hasMore: false,
  })),
}));

describe("scm-inspect commit-files pool", () => {
  it("caps concurrent gitCommitFiles while weighting timeline points", async () => {
    const { gitCommitFiles } = await import("../../services/backend");
    const spy = vi.mocked(gitCommitFiles);
    let inflight = 0;
    let maxInflight = 0;
    spy.mockImplementation(async () => {
      inflight += 1;
      maxInflight = Math.max(maxInflight, inflight);
      await new Promise((resolve) => setTimeout(resolve, 10));
      inflight -= 1;
      return [{ path: "a.ts", status: "M" }];
    });

    const points = Array.from({ length: 10 }, (_, index) => ({
      hash: `h${index}`,
      subject: `c${index}`,
      author: "a",
      at: Date.now() - index * 1000,
      weight: 1,
      refs: [] as string[],
    }));
    await attachTimelineWeights("ws", points, { concurrency: 3 });
    expect(maxInflight).toBeLessThanOrEqual(3);
    expect(spy).toHaveBeenCalledTimes(10);
  });

  it("stops loading frequencies when cancelled", async () => {
    const { gitCommitFiles } = await import("../../services/backend");
    const spy = vi.mocked(gitCommitFiles);
    let cancelled = false;
    spy.mockImplementation(async () => {
      await new Promise((resolve) => setTimeout(resolve, 5));
      cancelled = true;
      return [{ path: "a.ts", status: "M" }];
    });
    const freq = await loadCommitFrequencies("ws", null, {
      concurrency: 1,
      isCancelled: () => cancelled,
    });
    expect(spy.mock.calls.length).toBeLessThan(12);
    expect(freq.size).toBeGreaterThanOrEqual(0);
  });
});
