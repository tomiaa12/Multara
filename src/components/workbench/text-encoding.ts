/** 合法 UTF-8 字符超过约 80% 时按 UTF-8 容错，避免个别非法字节把整篇中文按 GB18030 解成乱码。 */
const UTF8_VALID_RATIO = 4 / 5;

function stripUtf8Bom(text: string) {
  return text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
}

function isLikelyBinary(bytes: Uint8Array) {
  const limit = Math.min(bytes.length, 8192);
  for (let i = 0; i < limit; i += 1) {
    if (bytes[i] === 0) return true;
  }
  return false;
}

/** 与后端 files::decode_text 一致：UTF-16 BOM → UTF-8 → 近 UTF-8 容错 → GB18030。 */
export function decodeTextBytes(bytes: Uint8Array): string {
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return new TextDecoder("utf-16le").decode(bytes.subarray(2));
  }
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xff) {
    return new TextDecoder("utf-16be").decode(bytes.subarray(2));
  }
  if (isLikelyBinary(bytes)) {
    throw new Error("暂不支持编辑二进制文件");
  }
  try {
    return stripUtf8Bom(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    const lossy = new TextDecoder("utf-8").decode(bytes);
    const chars = [...lossy];
    const replacements = chars.filter((char) => char === "\uFFFD").length;
    if (replacements > 0 && (chars.length - replacements) / Math.max(chars.length, 1) >= UTF8_VALID_RATIO) {
      return stripUtf8Bom(lossy);
    }
    return new TextDecoder("gb18030").decode(bytes);
  }
}
