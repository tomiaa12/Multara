import { describe, expect, it } from "vitest";
import { resolveLocale, translate } from "./i18n";

describe("i18n", () => {
  it("resolves East Asian and English locales", () => {
    expect(resolveLocale("zh-CN")).toBe("zh-CN");
    expect(resolveLocale("zh-TW")).toBe("zh-TW");
    expect(resolveLocale("zh-Hant")).toBe("zh-TW");
    expect(resolveLocale("zh-HK")).toBe("zh-TW");
    expect(resolveLocale("ja-JP")).toBe("ja");
    expect(resolveLocale("ko-KR")).toBe("ko");
    expect(resolveLocale("en-US")).toBe("en");
    expect(resolveLocale("fr")).toBe("en");
  });

  it("falls back to English catalogs for missing keys", () => {
    expect(translate("nav.runtimes", "en")).toBe("Runtimes");
    expect(translate("nav.runtimes", "zh-CN")).toBe("运行时");
    expect(translate("nav.runtimes", "ja")).toBe("ランタイム");
    expect(translate("nav.runtimes", "ko")).toBe("런타임");
  });

  it("interpolates variables", () => {
    expect(translate("app.runningCount", "en", { n: 3 })).toBe("3 running");
    expect(translate("git.folderDirty", "zh-CN", { count: 2 })).toBe("目录下有 2 处改动");
  });
});
