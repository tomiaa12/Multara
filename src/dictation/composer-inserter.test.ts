// @vitest-environment happy-dom
import { describe, expect, test } from "vitest";
import { createComposerInserter } from "./composer-inserter";

function host() {
  const root = document.createElement("div");
  document.body.append(root);
  let changes = 0;
  return {
    root,
    changes: () => changes,
    host: { getEditor: () => root, notifyChange: () => { changes += 1; }, isComposing: () => false },
  };
}

describe("composer inserter", () => {
  test("dispose keeps the dictated text and removes the marker so the next session does not reuse it", () => {
    const { root, host: h } = host();
    const first = createComposerInserter(h, { language: "en-US" });
    first.update("first dictation", "first");
    first.dispose();
    expect(root.querySelector("span[data-dictation]")).toBeNull();
    expect(root.querySelector(".dictation-interim")).toBeNull();
    expect(root.textContent).toBe("first dictation");

    const second = createComposerInserter(h, { language: "en-US" });
    second.update("second", "second");
    second.finish();
    expect(root.textContent).toBe("first dictation second");
  });

  test("finish leaves plain text with no grey interim span", () => {
    const { root, host: h } = host();
    const inserter = createComposerInserter(h, { language: "zh-CN" });
    inserter.update("你好世界", "你好");
    expect(root.querySelector(".dictation-interim")?.textContent).toBe("世界");
    inserter.finish();
    expect(root.querySelector("[class*=dictation]")).toBeNull();
    expect(root.textContent).toBe("你好世界");
  });

  test("editing the dictated text keeps it, drops the grey style, and later words are appended", () => {
    const { root, host: h } = host();
    const inserter = createComposerInserter(h, { language: "zh-CN" });
    inserter.update("你好世界", "你好");
    root.dispatchEvent(new Event("input"));
    expect(root.querySelector("[class*=dictation]")).toBeNull();
    expect(inserter.userModified).toBe(true);
    inserter.update("你好世界再见", "你好世界");
    expect(root.textContent).toBe("你好世界再见");
  });
});
