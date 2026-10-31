/** Legacy DOM keyCode while an IME is processing the key (DOM_VK_PROCESS). */
const IME_PROCESS_KEY_CODE = 229;
/** Hold the confirm flag until the next task, after WebKit's trailing Enter keydown. */
const CONFIRM_ENTER_HOLD_MS = 0;

export type ComposerEnter = "ime" | "confirm" | "submit";

/**
 * Tracks CJK composition so Enter confirms a candidate instead of sending.
 *
 * WKWebView dispatches the committing Enter after `compositionend`, with
 * `isComposing` already false. The confirm flag stays set until the next task
 * so that keydown is still treated as an IME commit.
 */
export function createComposerIme() {
  let composing = false;
  let confirmEnter = false;
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  function clearConfirmTimer() {
    if (confirmTimer === undefined) return;
    clearTimeout(confirmTimer);
    confirmTimer = undefined;
  }

  return {
    compositionStart() {
      clearConfirmTimer();
      composing = true;
      confirmEnter = false;
    },
    compositionEnd() {
      composing = false;
      confirmEnter = true;
      clearConfirmTimer();
      confirmTimer = setTimeout(() => {
        confirmEnter = false;
        confirmTimer = undefined;
      }, CONFIRM_ENTER_HOLD_MS);
    },
    dispose() {
      clearConfirmTimer();
      composing = false;
      confirmEnter = false;
    },
    isComposing() {
      return composing;
    },
    classifyEnter(event: Pick<KeyboardEvent, "isComposing" | "keyCode">): ComposerEnter {
      if (composing || event.isComposing || event.keyCode === IME_PROCESS_KEY_CODE) return "ime";
      if (confirmEnter) return "confirm";
      return "submit";
    },
  };
}
