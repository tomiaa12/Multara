// @vitest-environment node
import { describe, expect, test } from "vitest";
import { activeComposerTrigger, replaceTriggerWithReference } from "./composer-entities";

describe("composer entity references", () => {
  test.each([
    ["@", "agent"],
    ["/", "skill"],
    ["#", "task"],
  ] as const)("opens %s on its default category", (symbol, category) => {
    expect(activeComposerTrigger(`请处理 ${symbol}mik`)).toEqual({
      symbol,
      category,
      query: "mik",
    });
  });

  test("replaces only the active token with a stable reference chip", () => {
    const trigger = activeComposerTrigger("请处理 #登录");
    expect(trigger).not.toBeNull();
    if (!trigger) return;
    expect(replaceTriggerWithReference(
      [{ type: "text", value: "请处理 #登录" }],
      trigger,
      { id: "task-7", kind: "task", label: "登录异常", group: "tasks" },
    )).toEqual([
      { type: "text", value: "请处理 " },
      {
        type: "chip",
        id: "task:task-7",
        referenceId: "task-7",
        referenceKind: "task",
        label: "登录异常",
      },
      { type: "text", value: " " },
    ]);
  });
});
