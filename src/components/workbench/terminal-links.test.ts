// @vitest-environment node
import { describe, expect, it } from "vitest";
import { detectFileLinks, detectUrlLinks } from "./terminal-links";

describe("detectFileLinks", () => {
  it("finds relative paths with line and column", () => {
    const [link] = detectFileLinks("src/App.vue:12:5 - error TS2322");
    expect(link).toMatchObject({ start: 0, end: 16, path: "src/App.vue", line: 12, column: 5 });
  });

  it("finds windows absolute paths and parenthesised positions", () => {
    const text = "H:\\proj\\src\\main.ts(40,7): error";
    expect(detectFileLinks(text)[0]).toMatchObject({ path: "H:\\proj\\src\\main.ts", line: 40, column: 7 });
  });

  it("reads python traceback lines", () => {
    const text = '  File "/srv/app.py", line 9, in <module>';
    expect(detectFileLinks(text)).toContainEqual(expect.objectContaining({ path: "/srv/app.py", line: 9 }));
  });

  it("skips urls, versions and plain words", () => {
    expect(detectFileLinks("see https://example.com/a/b.js now")).toEqual([]);
    expect(detectFileLinks("version 1.2.3 ready")).toEqual([]);
    expect(detectFileLinks("hello world")).toEqual([]);
  });

  it("drops a trailing sentence period", () => {
    expect(detectFileLinks("wrote ./out/report.txt.")[0]).toMatchObject({ path: "./out/report.txt" });
  });
});

describe("detectUrlLinks", () => {
  it("keeps the address and drops trailing punctuation", () => {
    expect(detectUrlLinks("see https://example.com/a/b.js.")).toEqual([
      { start: 4, end: 30, url: "https://example.com/a/b.js" },
    ]);
  });
});
