import { describe, expect, it, vi } from "vitest";
import { runQuickAction } from "./workflow-gate-action";

const gate = (requireComment: boolean) => ({
  label: "打回", prompt: "打回修改",
  action: { type: "workflow.gate", nodeRunId: "nr1", decision: "changes", requireComment, form: { a: 1 } },
});

describe("runQuickAction", () => {
  it("calls decide for gate actions without comment prompt", async () => {
    const decide = vi.fn().mockResolvedValue(undefined);
    const promptComment = vi.fn();
    const result = await runQuickAction(gate(false), { decide, promptComment });
    expect(result).toBe("done");
    expect(promptComment).not.toHaveBeenCalled();
    expect(decide).toHaveBeenCalledWith("nr1", "changes", "", { a: 1 });
  });

  it("prompts for a comment when required and supports cancel", async () => {
    const decide = vi.fn().mockResolvedValue(undefined);
    expect(await runQuickAction(gate(true), { decide, promptComment: async () => "太丑" })).toBe("done");
    expect(decide).toHaveBeenCalledWith("nr1", "changes", "太丑", { a: 1 });
    decide.mockClear();
    expect(await runQuickAction(gate(true), { decide, promptComment: async () => null })).toBe("cancelled");
    expect(decide).not.toHaveBeenCalled();
  });

  it("reports failures", async () => {
    const onError = vi.fn();
    const result = await runQuickAction(gate(false), { decide: async () => { throw new Error("x"); }, promptComment: async () => "", onError });
    expect(result).toBe("failed");
    expect(onError).toHaveBeenCalled();
  });

  it("leaves normal quick actions to the chat path", async () => {
    const decide = vi.fn();
    const result = await runQuickAction({ label: "好", prompt: "好的" }, { decide, promptComment: async () => "" });
    expect(result).toBe("not-handled");
    expect(decide).not.toHaveBeenCalled();
  });
});
