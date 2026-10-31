/**
 * SpeechRecognition 在停顿后会结束，重启后的 `results` 从头开始。
 * 这里把每一轮识别的结果接到之前已提交的文字后面，保证对外的累计文本只增不减。
 */
export type SpeechResultLike = {
  isFinal: boolean;
  length?: number;
  0?: { transcript?: string };
};

const CJK_LANG_RE = /^(zh|ja|ko)([-_]|$)/i;

export function createSegmentAccumulator(language: string) {
  let committed = "";

  function join(base: string, next: string): string {
    if (!base || !next) return base + next;
    if (CJK_LANG_RE.test(language)) return base + next;
    if (/\s$/.test(base) || /^\s/.test(next)) return base + next;
    return `${base} ${next}`;
  }

  return {
    /** 读取本轮识别的结果，返回接在已提交文字之后的累计文本。 */
    read(results: ArrayLike<SpeechResultLike>): { text: string; finalizedText: string } {
      let finalPart = "";
      let interimPart = "";
      for (let i = 0; i < results.length; i += 1) {
        const item = results[i];
        const piece = item?.[0]?.transcript ?? "";
        if (!piece) continue;
        if (item.isFinal) finalPart += piece;
        else interimPart += piece;
      }
      const finalizedText = join(committed, finalPart);
      const text = interimPart ? join(finalizedText, interimPart) : finalizedText;
      return { text, finalizedText };
    },
    /** 引擎结束并将重启：把已有的文字（含未定稿部分）当作已提交。 */
    commit(text: string) {
      committed = text;
    },
    get committed() {
      return committed;
    },
  };
}
