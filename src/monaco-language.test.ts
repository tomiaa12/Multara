// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  AUTO_DETECT_LANGUAGE,
  applyLanguageOverride,
  filterSelectableLanguages,
  jsoncDiagnosticsOptions,
  languageDisplayName,
  languageIconName,
  listSelectableLanguages,
  matchesAutoDetect,
  resolveMonacoLanguageId,
  shouldSuppressEmbeddedDiagnostics,
} from "./monaco-language";

describe("resolveMonacoLanguageId", () => {
  const registered = (id: string) => ["json", "markdown", "html"].includes(id);

  test("keeps a registered language", () => {
    expect(resolveMonacoLanguageId("markdown", registered)).toBe("markdown");
  });

  test("maps jsonc onto json so tsconfig keeps highlighting", () => {
    expect(resolveMonacoLanguageId("jsonc", registered)).toBe("json");
  });

  test("falls back to plaintext when the language is unknown", () => {
    expect(resolveMonacoLanguageId("unknown", registered)).toBe("plaintext");
  });
});

describe("jsonc diagnostics", () => {
  test("ignores comments and trailing commas like VS Code jsonc", () => {
    expect(jsoncDiagnosticsOptions.comments).toBe("ignore");
    expect(jsoncDiagnosticsOptions.trailingCommas).toBe("ignore");
    expect(jsoncDiagnosticsOptions.allowComments).toBe(true);
  });
});

describe("embedded diagnostics", () => {
  test("suppresses language-service markers inside markdown", () => {
    expect(shouldSuppressEmbeddedDiagnostics("markdown")).toBe(true);
    expect(shouldSuppressEmbeddedDiagnostics("mdx")).toBe(true);
    expect(shouldSuppressEmbeddedDiagnostics("json")).toBe(false);
    expect(shouldSuppressEmbeddedDiagnostics("html")).toBe(false);
    expect(shouldSuppressEmbeddedDiagnostics("html", "src/App.vue")).toBe(true);
    expect(shouldSuppressEmbeddedDiagnostics("html", "/ws/src/App.vue")).toBe(true);
    expect(shouldSuppressEmbeddedDiagnostics("html", "src/index.html")).toBe(false);
  });
});

describe("language picker", () => {
  const languages = [
    { id: "typescript", aliases: ["TypeScript", "ts"] },
    { id: "markdown", aliases: ["Markdown"] },
    { id: "json", aliases: ["JSON"] },
  ];

  test("uses the first alias as the status bar label", () => {
    expect(languageDisplayName("typescript", languages)).toBe("TypeScript");
    expect(languageDisplayName(AUTO_DETECT_LANGUAGE, languages)).toBe("自动检测");
  });

  test("lists languages alphabetically by display name", () => {
    expect(listSelectableLanguages(languages).map((item) => item.label)).toEqual([
      "JSON",
      "Markdown",
      "TypeScript",
    ]);
  });

  test("filters by label or language id", () => {
    const options = listSelectableLanguages(languages);
    expect(filterSelectableLanguages(options, "script").map((item) => item.id)).toEqual([
      "typescript",
    ]);
    expect(filterSelectableLanguages(options, "ts").map((item) => item.id)).toEqual(["typescript"]);
    expect(filterSelectableLanguages(options, "mark").map((item) => item.id)).toEqual(["markdown"]);
  });

  test("keeps auto-detect visible for empty or matching queries", () => {
    expect(matchesAutoDetect("")).toBe(true);
    expect(matchesAutoDetect("自动")).toBe(true);
    expect(matchesAutoDetect("rust")).toBe(false);
  });

  test("maps a language to the same file name the tree icons use", () => {
    expect(languageIconName({ id: "typescript", extensions: [".ts"] })).toBe("file.ts");
    expect(languageIconName({ id: "dockerfile", filenames: ["Dockerfile"] })).toBe("Dockerfile");
    expect(languageIconName({ id: "markdown" })).toBe("file.md");
    expect(languageIconName({ id: AUTO_DETECT_LANGUAGE })).toBe("file");
  });

  test("applies a per-file language override", () => {
    const file = applyLanguageOverride(
      { workspaceId: "ws", path: "README", language: "plaintext" },
      { "ws:README": "markdown" },
    );
    expect(file.language).toBe("markdown");
  });
});
