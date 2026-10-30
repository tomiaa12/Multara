// @vitest-environment node
import { describe, expect, test } from "vitest";
import { EMMET_MAX_REPEAT, EMMET_SCAN_LIMIT, emmetTrigger, expandEmmet } from "./emmet";

describe("emmet", () => {
  test("expands html tags with class and child", () => {
    expect(expandEmmet("div.card", "html")).toBe('<div class="card">$0</div>');
    expect(expandEmmet("ul>li", "html")).toBe("<ul><li>$0</li></ul>");
    expect(emmetTrigger("  div.card")).toBe("div.card");
  });

  test("expands css abbreviations", () => {
    expect(expandEmmet("m-10", "css")).toBe("margin: 10px;");
    expect(expandEmmet("d-f", "css")).toBe("display: flex;");
  });

  test("caps the repeat count", () => {
    expect(expandEmmet(`li*${EMMET_MAX_REPEAT}`, "html")?.split("\n")).toHaveLength(EMMET_MAX_REPEAT);
    expect(expandEmmet(`li*${EMMET_MAX_REPEAT + 1}`, "html")).toBeNull();
    expect(expandEmmet("li*99999999", "html")).toBeNull();
  });

  test("only scans a bounded tail before the cursor", () => {
    expect(emmetTrigger(`${"x".repeat(EMMET_SCAN_LIMIT * 4)} ul>li`)).toBe("ul>li");
    expect(emmetTrigger("a".repeat(EMMET_SCAN_LIMIT + 1))).toBeNull();
  });
});
