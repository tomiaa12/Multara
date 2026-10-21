// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  rankTerminalSuggestions,
  suggestionInsertText,
  terminalCompletionDirectory,
  terminalInputToken,
} from "./terminal-suggest";

describe("terminalInputToken", () => {
  test("reads the command being typed after a prompt", () => {
    expect(terminalInputToken("user % no", 10)).toEqual({
      kind: "command",
      token: "no",
      directory: "",
      name: "no",
    });
  });

  test("reads a path token after the command", () => {
    expect(terminalInputToken("% node src/vi", 14)).toMatchObject({
      kind: "path",
      name: "vi",
      directory: "src/",
    });
  });

  test("ignores a line that has no prompt", () => {
    expect(terminalInputToken("compiled output", 8)).toBeNull();
  });

  test("understands PowerShell and cmd prompts", () => {
    expect(terminalInputToken("PS C:\\work> gi", 14)).toMatchObject({ kind: "command", name: "gi" });
    expect(terminalInputToken("C:\\work>cd src\\com", 18)).toMatchObject({ kind: "path", directory: "src\\", name: "com" });
  });

  test("uses the shell integration input start when known", () => {
    expect(terminalInputToken("weird ~~ prompt npm r", 21, 16)).toMatchObject({ kind: "path", name: "r" });
  });
});

describe("rankTerminalSuggestions", () => {
  test("keeps names that start with the typed prefix", () => {
    expect(rankTerminalSuggestions(["node", "npm", "deno", "nodemon"], "no")).toEqual([
      "node",
      "nodemon",
      "deno",
    ]);
  });

  test("lists prefix matches before substring matches", () => {
    expect(rankTerminalSuggestions([
      { name: "my-git", kind: "command" as const },
      { name: "git", kind: "command" as const },
      { name: "gitk", kind: "command" as const },
    ], "gi").map((item) => item.name)).toEqual(["git", "gitk", "my-git"]);
  });
});

describe("terminalCompletionDirectory", () => {
  test("joins relative directories with the platform separator", () => {
    expect(terminalCompletionDirectory("C:\\work\\", "src/", true)).toBe("C:\\work\\src\\");
    expect(terminalCompletionDirectory("/home/me", "src/", false)).toBe("/home/me/src/");
    expect(terminalCompletionDirectory("C:\\work", "D:\\x\\", true)).toBe("D:\\x\\");
  });
});

describe("suggestionInsertText", () => {
  test("appends the rest of the name and a separator for folders", () => {
    expect(suggestionInsertText("sr", { name: "src", kind: "folder" }, false)).toBe("c/");
    expect(suggestionInsertText("REA", { name: "README.md", kind: "file" }, true)).toBe("DME.md");
  });

  test("retypes the word when only the case differs", () => {
    expect(suggestionInsertText("readme", { name: "README.md", kind: "file" }, true)).toBe("\x7f".repeat(6) + "README.md");
  });
});
