// @vitest-environment node
import { describe, expect, test } from "vitest";
import { terminalKeyCommand, terminalSendSequence } from "./terminal-keys";

function key(partial: Partial<KeyboardEvent>): KeyboardEvent {
  return {
    key: "",
    code: "",
    altKey: false,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    isComposing: false,
    repeat: false,
    ...partial,
  } as KeyboardEvent;
}

describe("terminalSendSequence", () => {
  test("mac option backspace deletes the word and command backspace deletes to the line start", () => {
    expect(terminalSendSequence(key({ key: "Backspace", altKey: true }), true)).toBe("\u0017");
    expect(terminalSendSequence(key({ key: "Backspace", metaKey: true }), true)).toBe("\u0015");
  });

  test("other platforms use ctrl backspace for the word and ctrl delete for the word on the right", () => {
    expect(terminalSendSequence(key({ key: "Backspace", ctrlKey: true }), false)).toBe("\u0017");
    expect(terminalSendSequence(key({ key: "Delete", ctrlKey: true }), false)).toBe("\u001bd");
  });

  test("mac command left and right jump to the line ends", () => {
    expect(terminalSendSequence(key({ key: "ArrowLeft", metaKey: true }), true)).toBe("\u0001");
    expect(terminalSendSequence(key({ key: "ArrowRight", metaKey: true }), true)).toBe("\u0005");
  });
});

describe("terminalKeyCommand", () => {
  test("ctrl c copies only when there is a selection, otherwise it reaches the shell", () => {
    const ctrlC = key({ key: "c", code: "KeyC", ctrlKey: true });
    expect(terminalKeyCommand(ctrlC, { mac: false, hasSelection: true })).toBe("copy");
    expect(terminalKeyCommand(ctrlC, { mac: false, hasSelection: false })).toBeNull();
    expect(terminalKeyCommand(ctrlC, { mac: true, hasSelection: true })).toBeNull();
  });

  test("windows pastes with ctrl v and ctrl shift v", () => {
    expect(terminalKeyCommand(key({ code: "KeyV", ctrlKey: true }), { mac: false })).toBe("paste");
    expect(terminalKeyCommand(key({ code: "KeyV", ctrlKey: true, shiftKey: true }), { mac: false })).toBe("paste");
  });

  test("maps navigation keys per platform", () => {
    expect(terminalKeyCommand(key({ key: "ArrowUp", ctrlKey: true }), { mac: false })).toBe("previousCommand");
    expect(terminalKeyCommand(key({ key: "ArrowUp", metaKey: true }), { mac: true })).toBe("previousCommand");
    expect(terminalKeyCommand(key({ key: "PageDown", shiftKey: true }), { mac: false })).toBe("scrollPageDown");
    expect(terminalKeyCommand(key({ code: "Digit5", ctrlKey: true, shiftKey: true }), { mac: false })).toBe("split");
    expect(terminalKeyCommand(key({ code: "Space", ctrlKey: true }), { mac: true })).toBe("suggest");
  });

  test("leaves ordinary typing alone", () => {
    expect(terminalKeyCommand(key({ key: "a", code: "KeyA" }), { mac: false })).toBeNull();
    expect(terminalKeyCommand(key({ key: "a", code: "KeyA", ctrlKey: true }), { mac: false })).toBeNull();
  });
});
