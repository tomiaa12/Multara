// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  compilerOptionsFromTsconfig,
  mergeWorkspaceCompilerOptions,
  parseTsconfigJson,
} from "./tsconfig-options";

describe("tsconfig options", () => {
  test("reads compilerOptions and ignores comments", () => {
    const parsed = parseTsconfigJson(`{
      // app
      "compilerOptions": {
        "strict": true,
        "jsx": "preserve",
        "baseUrl": ".",
        "paths": { "@/*": ["src/*"] },
      }
    }`);
    expect(compilerOptionsFromTsconfig(parsed)).toEqual({
      jsx: "preserve",
      strict: true,
      baseUrl: ".",
      paths: { "@/*": ["src/*"] },
    });
  });

  test("rewrites aliases of every workspace under the virtual root", () => {
    expect(
      mergeWorkspaceCompilerOptions([
        { workspaceId: "a", options: { strict: true, baseUrl: ".", paths: { "@/*": ["./src/*"] } } },
        { workspaceId: "b", options: { paths: { "@/*": ["web/../app/*"] } } },
        { workspaceId: "c", options: { baseUrl: "src" } },
        { workspaceId: "d", options: { jsx: "preserve" } },
      ]),
    ).toEqual({
      strict: true,
      jsx: "preserve",
      baseUrl: "file:///",
      paths: {
        "@/*": ["a/src/*", "b/app/*"],
        "*": ["c/src/*"],
      },
    });
  });

  test("leaves module resolution alone without aliases", () => {
    expect(mergeWorkspaceCompilerOptions([{ workspaceId: "a", options: { strict: false } }])).toEqual({
      strict: false,
    });
  });
});
