import { describe, expect, it } from "vitest";
import { decodeTerminalChunk } from "./terminal-decode";

describe("decodeTerminalChunk", () => {
  it("decodes base64 into raw bytes", () => {
    expect([...decodeTerminalChunk(btoa("hi\r\n"))]).toEqual([104, 105, 13, 10]);
  });

  it("keeps bytes above 0x7f intact", () => {
    expect([...decodeTerminalChunk(btoa(String.fromCharCode(0, 0x80, 0xff)))]).toEqual([0, 0x80, 0xff]);
  });

  it("reuses the decoded block for the same payload", () => {
    const data = btoa("same chunk");
    expect(decodeTerminalChunk(data)).toBe(decodeTerminalChunk(data));
  });

  it("returns an empty array for malformed input", () => {
    expect(decodeTerminalChunk("%%%").length).toBe(0);
  });
});
