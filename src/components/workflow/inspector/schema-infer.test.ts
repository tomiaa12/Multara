import { describe, expect, it } from "vitest";
import { inferSchema, parseJsonText } from "./schema-infer";

describe("schema-infer", () => {
  it("infers nested objects and arrays", () => {
    expect(inferSchema({ a: 1, b: "x", c: [true], d: 1.5 })).toEqual({
      type: "object",
      properties: { a: { type: "integer" }, b: { type: "string" }, c: { type: "array", items: { type: "boolean" } }, d: { type: "number" } },
      required: ["a", "b", "c", "d"],
    });
  });
  it("parses json text", () => {
    expect(parseJsonText("")).toBeUndefined();
    expect(parseJsonText("{")).toBeInstanceOf(Error);
    expect(parseJsonText("[1]")).toEqual([1]);
  });
});
