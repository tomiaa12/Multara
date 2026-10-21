import { describe, expect, it } from "vitest";
import { directoryColumnTemplate, type DirectoryColumn } from "./directory-table";

describe("directoryColumnTemplate", () => {
  it("joins column sizes for CSS grid", () => {
    const columns: DirectoryColumn[] = [
      { key: "a", size: "112px" },
      { key: "b", size: "minmax(0, 1fr)" },
      { key: "c", size: "minmax(320px, 1.1fr)", actions: true },
    ];
    expect(directoryColumnTemplate(columns)).toBe("112px minmax(0, 1fr) minmax(320px, 1.1fr)");
  });

  it("ignores empty input", () => {
    expect(directoryColumnTemplate(undefined)).toBe("");
    expect(directoryColumnTemplate([{ key: "x", size: "  " }])).toBe("");
  });
});
