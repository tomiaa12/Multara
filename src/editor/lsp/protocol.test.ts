// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  fileUriFromFsPath,
  fsPathFromFileUri,
  normalizeDefinitionResult,
  replyToServerRequest,
  supportsIncrementalSync,
} from "./protocol";

const range = { start: { line: 1, character: 2 }, end: { line: 1, character: 5 } };
const selection = { start: { line: 3, character: 4 }, end: { line: 3, character: 8 } };

describe("lsp protocol", () => {
  test("normalizes every definition result shape", () => {
    expect(normalizeDefinitionResult(null)).toEqual([]);
    expect(normalizeDefinitionResult({ uri: "file:///a", range })).toEqual([{ uri: "file:///a", range }]);
    expect(
      normalizeDefinitionResult([{ targetUri: "file:///b", targetRange: range, targetSelectionRange: selection }]),
    ).toEqual([{ uri: "file:///b", range: selection }]);
  });

  test("converts file uris from different servers to the same path", () => {
    expect(fsPathFromFileUri("file:///h%3A/%E8%B7%A8%E7%AB%AF/a.rs")).toBe("H:/跨端/a.rs");
    expect(fsPathFromFileUri("file:///H:/repo/a.rs")).toBe("H:/repo/a.rs");
    expect(fsPathFromFileUri("file:///home/me/a%20b.py")).toBe("/home/me/a b.py");
    expect(fsPathFromFileUri("untitled:1")).toBeNull();
  });

  test("builds uris that round-trip", () => {
    const uri = fileUriFromFsPath("H:\\跨端应用\\Multara\\src\\a b.rs");
    expect(uri).toBe("file:///H:/%E8%B7%A8%E7%AB%AF%E5%BA%94%E7%94%A8/Multara/src/a%20b.rs");
    expect(fsPathFromFileUri(uri)).toBe("H:/跨端应用/Multara/src/a b.rs");
    expect(fileUriFromFsPath("/home/me/a.py")).toBe("file:///home/me/a.py");
  });

  test("answers server requests", () => {
    expect(replyToServerRequest("workspace/configuration", { items: [{}, {}] }, [])).toEqual({
      result: [null, null],
    });
    expect(replyToServerRequest("custom/x", undefined, [])).toHaveProperty("error");
    expect(replyToServerRequest("workspace/applyEdit", {}, [])).toEqual({ result: { applied: false } });
  });

  test("reads the incremental sync capability in both shapes", () => {
    expect(supportsIncrementalSync({ capabilities: { textDocumentSync: 2 } })).toBe(true);
    expect(supportsIncrementalSync({ capabilities: { textDocumentSync: { change: 2 } } })).toBe(true);
    expect(supportsIncrementalSync({ capabilities: { textDocumentSync: 1 } })).toBe(false);
    expect(supportsIncrementalSync(null)).toBe(false);
  });
});
