// @vitest-environment node
import { describe, expect, test } from "vitest";
import { emmetTrigger, expandEmmet } from "./emmet";

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
});
