import { describe, expect, it } from "vitest";
import { isCjkDictationLanguage, resolveDictationLanguage } from "./dictation-language";

describe("resolveDictationLanguage", () => {
  it("returns configured locale", () => {
    expect(resolveDictationLanguage("zh-CN", "en", "en-US")).toBe("zh-CN");
  });

  it("maps auto to display language", () => {
    expect(resolveDictationLanguage("auto", "ja", "en-US")).toBe("ja-JP");
  });

  it("falls back to browser language", () => {
    expect(resolveDictationLanguage("auto", undefined, "ko-KR")).toBe("ko-KR");
  });
});

describe("isCjkDictationLanguage", () => {
  it("detects cjk tags", () => {
    expect(isCjkDictationLanguage("zh-CN")).toBe(true);
    expect(isCjkDictationLanguage("en-US")).toBe(false);
  });
});
