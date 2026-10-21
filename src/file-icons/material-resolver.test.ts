// @vitest-environment node
import { describe, expect, it } from "vitest";
import manifest from "material-icon-theme/dist/material-icons.json";
import { createMaterialResolver, type MaterialManifest } from "./material-resolver";

const resolve = createMaterialResolver(
  manifest as MaterialManifest,
  (iconPath) => iconPath.slice(iconPath.lastIndexOf("/") + 1),
);
const icon = (query: Parameters<typeof resolve>[0]) => {
  const result = resolve(query);
  return "src" in result ? result.src : result.iconify;
};

describe("createMaterialResolver", () => {
  it("matches exact file names before extensions", () => {
    expect(icon({ name: "package.json" })).toBe("nodejs.svg");
    expect(icon({ name: "tsconfig.json" })).toBe("tsconfig.svg");
  });

  it("is case-insensitive", () => {
    expect(icon({ name: "Dockerfile" })).toBe("docker.svg");
    expect(icon({ name: "App.VUE" })).toBe("vue.svg");
  });

  it("prefers the longest compound extension", () => {
    expect(icon({ name: "foo.spec.ts" })).not.toBe(icon({ name: "foo.ts" }));
    expect(icon({ name: "foo.ts" })).toBe("typescript.svg");
  });

  it("falls back to the default file and folder icons", () => {
    expect(icon({ name: "unknown.zzzz" })).toBe("file.svg");
    expect(icon({ name: "random-folder", directory: true })).toBe("folder.svg");
    expect(icon({ name: "random-folder", directory: true, open: true })).toBe("folder-open.svg");
  });

  it("distinguishes collapsed and expanded named folders", () => {
    expect(icon({ name: "src", directory: true })).toBe("folder-src.svg");
    expect(icon({ name: "src", directory: true, open: true })).toBe("folder-src-open.svg");
  });

  it("uses light overrides only in light mode", () => {
    expect(icon({ name: "Cargo.toml" })).toBe(icon({ name: "Cargo.toml", light: false }));
    expect(icon({ name: "config.toml", light: true })).toBe("toml_light.svg");
    expect(icon({ name: "config.toml" })).toBe("toml.svg");
  });
});
