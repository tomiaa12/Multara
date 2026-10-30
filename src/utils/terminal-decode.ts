const EMPTY = new Uint8Array();

let lastEncoded = "";
let lastDecoded = EMPTY;

/** 终端面板和运行中终端仓库各自监听同一个 `terminal-data` 事件，缓存最近一块，第二个监听者直接复用。返回值是共享的，调用方不能原地修改。 */
export function decodeTerminalChunk(data: string) {
  if (data === lastEncoded) return lastDecoded;
  try {
    const binary = atob(data);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    lastEncoded = data;
    lastDecoded = bytes;
    return bytes;
  } catch {
    return EMPTY;
  }
}
