import { describe, expect, test } from "vitest";
import { decodeTextBytes } from "./text-encoding";

describe("decodeTextBytes", () => {
  test("reads utf-8 chinese", () => {
    expect(decodeTextBytes(new TextEncoder().encode("# 安装文档\n请先安装"))).toBe("# 安装文档\n请先安装");
  });

  test("keeps utf-8 chinese when a few bytes are invalid", () => {
    const bytes = new Uint8Array([...new TextEncoder().encode("安装文档\n请先安装"), 0x80]);
    const text = decodeTextBytes(bytes);
    expect(text).toContain("安装文档");
    expect(text).toContain("请先安装");
  });

  test("reads gbk chinese", () => {
    expect(decodeTextBytes(new Uint8Array([0xc4, 0xe3, 0xba, 0xc3, 0x0a]))).toBe("你好\n");
  });
});
