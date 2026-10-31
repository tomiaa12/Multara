const SUPPORTED = new Set([
  "zh-CN",
  "zh-TW",
  "en-US",
  "en-GB",
  "ja-JP",
  "ko-KR",
]);

const BY_LANGUAGE: Record<string, string> = {
  zh: "zh-CN",
  en: "en-US",
  ja: "ja-JP",
  ko: "ko-KR",
};

/**
 * Resolve SpeechRecognition `lang`. `auto` follows UI / browser locale when
 * supported; empty string lets the engine pick.
 */
export function resolveDictationLanguage(
  configured: string | undefined,
  displayLanguage?: string,
  browserLanguage = typeof navigator !== "undefined" ? navigator.language : "en",
): string {
  const raw = configured?.trim() ?? "";
  if (raw && raw.toLowerCase() !== "auto") {
    return canonicalize(raw) ?? raw;
  }
  return canonicalize(displayLanguage) ?? canonicalize(browserLanguage) ?? "";
}

function canonicalize(candidate: string | undefined): string | undefined {
  if (!candidate) return undefined;
  try {
    const canonical = Intl.getCanonicalLocales(candidate)[0];
    if (!canonical) return undefined;
    if (SUPPORTED.has(canonical)) return canonical;
    if (canonical.toLowerCase().startsWith("zh-hant") || canonical === "zh-TW" || canonical === "zh-HK") {
      return "zh-TW";
    }
    if (canonical.toLowerCase().startsWith("zh")) return "zh-CN";
    return BY_LANGUAGE[canonical.split("-")[0] ?? ""];
  } catch {
    return undefined;
  }
}

export function isCjkDictationLanguage(lang: string): boolean {
  return /^(zh|ja|ko)([-_]|$)/i.test(lang);
}
