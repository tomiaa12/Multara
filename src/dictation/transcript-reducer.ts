export type DictationTranscript = {
  revision: number;
  text: string;
  finalizedText?: string;
  isFinal: boolean;
};

export type TranscriptReducerState = {
  revision: number;
  text: string;
  finalizedText: string;
  isFinal: boolean;
};

export function createTranscriptReducer(): {
  apply: (next: DictationTranscript) => TranscriptReducerState | null;
  snapshot: () => TranscriptReducerState;
  reset: () => void;
} {
  let state: TranscriptReducerState = {
    revision: 0,
    text: "",
    finalizedText: "",
    isFinal: false,
  };

  return {
    apply(next) {
      if (state.isFinal) return null;
      if (next.revision < state.revision) return null;
      const finalizedText = next.finalizedText ?? "";
      const mergedFinalized =
        finalizedText.length >= state.finalizedText.length
          ? finalizedText
          : state.finalizedText;
      state = {
        revision: next.revision,
        text: next.text,
        finalizedText: mergedFinalized,
        isFinal: next.isFinal,
      };
      return state;
    },
    snapshot: () => state,
    reset() {
      state = { revision: 0, text: "", finalizedText: "", isFinal: false };
    },
  };
}
