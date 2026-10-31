import { describe, expect, test } from "vitest";
import { createSegmentAccumulator } from "./segment-accumulator";

const r = (isFinal: boolean, transcript: string) => ({ isFinal, length: 1, 0: { transcript } });

describe("segment accumulator", () => {
  test("keeps earlier text after the recognizer restarts (CJK, no separator)", () => {
    const acc = createSegmentAccumulator("zh-CN");
    const first = acc.read([r(true, "你好"), r(false, "世界")]);
    expect(first).toEqual({ text: "你好世界", finalizedText: "你好" });
    acc.commit(first.text);
    expect(acc.read([r(false, "再见")])).toEqual({ text: "你好世界再见", finalizedText: "你好世界" });
    expect(acc.read([r(true, "再见")])).toEqual({ text: "你好世界再见", finalizedText: "你好世界再见" });
  });

  test("adds a space between restarts for Latin languages", () => {
    const acc = createSegmentAccumulator("en-US");
    const first = acc.read([r(true, "hello")]);
    acc.commit(first.text);
    expect(acc.read([r(true, "world")]).text).toBe("hello world");
    expect(acc.read([r(true, " world")]).text).toBe("hello world");
  });

  test("accumulated text never shrinks across several restarts", () => {
    const acc = createSegmentAccumulator("");
    let last = "";
    for (const piece of ["a", "b", "c"]) {
      const next = acc.read([r(true, piece)]);
      expect(next.text.startsWith(last)).toBe(true);
      last = next.text;
      acc.commit(next.text);
    }
    expect(last).toBe("a b c");
  });
});
