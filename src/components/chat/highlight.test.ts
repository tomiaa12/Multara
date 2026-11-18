import { describe, expect, it } from "vitest";
import { highlightCodeLine, highlightLanguageForPath } from "./highlight";

describe("highlightLanguageForPath", () => {
  it("maps common extensions to highlight.js languages", () => {
    expect(highlightLanguageForPath("src/app.ts")).toBe("typescript");
    expect(highlightLanguageForPath("a.vue")).toBe("xml");
    expect(highlightLanguageForPath("run.sh")).toBe("bash");
    expect(highlightLanguageForPath("notes.md")).toBe("markdown");
  });

  it("falls back to plaintext for unknown paths", () => {
    expect(highlightLanguageForPath("blob.bin")).toBe("plaintext");
  });
});

describe("highlightCodeLine", () => {
  it("emits token spans for keywords", () => {
    const html = highlightCodeLine("const x = 1;", "typescript");
    expect(html).toContain("hljs-keyword");
    expect(html).toContain("const");
  });

  it("escapes plain text when language is plaintext", () => {
    const html = highlightCodeLine("<script>", "plaintext");
    expect(html).toContain("&lt;script&gt;");
  });
});
