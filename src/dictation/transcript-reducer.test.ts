import { describe, expect, it } from "vitest";
import { createTranscriptReducer } from "./transcript-reducer";

describe("createTranscriptReducer", () => {
  it("drops out-of-order revisions", () => {
    const reducer = createTranscriptReducer();
    expect(reducer.apply({ revision: 2, text: "ab", finalizedText: "a", isFinal: false })?.text).toBe("ab");
    expect(reducer.apply({ revision: 1, text: "x", isFinal: false })).toBeNull();
    expect(reducer.snapshot().text).toBe("ab");
  });

  it("keeps finalizedText monotonic", () => {
    const reducer = createTranscriptReducer();
    reducer.apply({ revision: 1, text: "hello world", finalizedText: "hello ", isFinal: false });
    const next = reducer.apply({
      revision: 2,
      text: "hello wor",
      finalizedText: "hel",
      isFinal: false,
    });
    expect(next?.finalizedText).toBe("hello ");
  });

  it("ignores updates after isFinal", () => {
    const reducer = createTranscriptReducer();
    reducer.apply({ revision: 1, text: "done", finalizedText: "done", isFinal: true });
    expect(reducer.apply({ revision: 2, text: "more", isFinal: false })).toBeNull();
  });
});
