import { describe, expect, it } from "vitest";
import { applyCompletion, filterCompletions, flattenVariables, tokenAtCursor } from "./template-completion";

describe("template completion", () => {
  const vars = { inputs: { url: "x" }, nodes: { prd: { outputs: { spec: {} } } }, iteration: 0 };

  it("flattens variable tree", () => {
    const paths = flattenVariables(vars).map((item) => item.path);
    expect(paths).toContain("nodes.prd.outputs.spec");
    expect(paths).toContain("iteration");
  });

  it("finds token only inside template tags", () => {
    expect(tokenAtCursor("hello inp", 9)).toBeNull();
    const text = "hello {{ inputs.u";
    expect(tokenAtCursor(text, text.length)).toMatchObject({ query: "inputs.u", start: 9 });
    expect(tokenAtCursor("{{ a }} b", 9)).toBeNull();
    expect(tokenAtCursor("{{ ", 3)).toMatchObject({ query: "" });
  });

  it("treats whole text as expression", () => {
    expect(tokenAtCursor("inputs.v", 8, true)?.query).toBe("inputs.v");
  });

  it("filters and applies", () => {
    const items = flattenVariables(vars);
    expect(filterCompletions(items, "inputs")[0].path).toBe("inputs.url");
    const text = "a {{ inputs.u";
    const token = tokenAtCursor(text, text.length)!;
    expect(applyCompletion(text, token, "inputs.url").text).toBe("a {{ inputs.url }}");
    expect(applyCompletion("inp", { start: 0, end: 3, query: "inp" }, "inputs.url", true).text).toBe("inputs.url");
  });
});
