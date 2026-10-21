// @vitest-environment node
import { describe, expect, test } from "vitest";
import { findMergeConflicts, resolveMergeConflict } from "./merge-conflict";

const SAMPLE = [
  "before",
  "<<<<<<< HEAD",
  "        if (!data?.result || err) {",
  "          hideEntryLoading()",
  "=======",
  "        if (data?.result) {",
  ">>>>>>> feature",
  "after",
].join("\n");

describe("findMergeConflicts", () => {
  test("reads the current and incoming names around the markers", () => {
    expect(findMergeConflicts(SAMPLE)).toEqual([
      {
        headerLine: 2,
        splitterLine: 5,
        footerLine: 7,
        currentName: "HEAD",
        incomingName: "feature",
      },
    ]);
    expect(findMergeConflicts("plain\n")).toEqual([]);
  });

  test("stops when a second header appears before the footer", () => {
    const text = "<<<<<<< A\n<<<<<<< B\n=======\nx\n>>>>>>> C\n";
    expect(findMergeConflicts(text)).toEqual([]);
  });
});

describe("resolveMergeConflict", () => {
  const conflict = findMergeConflicts(SAMPLE)[0];

  test("keeps the current side and drops the markers", () => {
    expect(resolveMergeConflict(SAMPLE, conflict, "current")).toBe([
      "before",
      "        if (!data?.result || err) {",
      "          hideEntryLoading()",
      "after",
    ].join("\n"));
  });

  test("keeps the incoming side", () => {
    expect(resolveMergeConflict(SAMPLE, conflict, "incoming")).toBe([
      "before",
      "        if (data?.result) {",
      "after",
    ].join("\n"));
  });

  test("keeps both sides without a blank line between them", () => {
    expect(resolveMergeConflict(SAMPLE, conflict, "both")).toBe([
      "before",
      "        if (!data?.result || err) {",
      "          hideEntryLoading()",
      "        if (data?.result) {",
      "after",
    ].join("\n"));
  });

  test("ignores the common ancestor block when keeping current", () => {
    const text = ["<<<<<<< HEAD", "ours", "||||||| base", "base", "=======", "theirs", ">>>>>>>"].join("\n");
    const [item] = findMergeConflicts(text);
    expect(resolveMergeConflict(text, item, "current")).toBe("ours");
    expect(resolveMergeConflict(text, item, "incoming")).toBe("theirs");
  });
});
