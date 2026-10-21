/*
 * 拼音模糊匹配（无字典依赖）
 *
 * 首字母：基于 Intl.Collator 的中文拼音排序规则推算；
 * 全拼：需要调用方提供按汉字以空格分隔的拼音（如 "ce shi"）。
 */

// 原理：在「拼音排序」规则下，每个声母区间取排在最前面的汉字作为分界；
// 某个汉字落在哪两个分界字之间，就属于哪个首字母。
// 分界字经过逐个校验（如 M 段用「妈」、T 段用「他」），改动需重新验证。
// 注：没有以 I / U / V 开头的汉字拼音，因此字母表跳过这三个。
const PINYIN_LETTERS = "ABCDEFGHJKLMNOPQRSTWXYZ";
const PINYIN_BOUNDARY = "阿八嚓哒妸发旮哈讥咔垃妈拏噢妑七呥扨他穵夕丫帀";

const collator = new Intl.Collator("zh-Hans-CN-u-co-pinyin");
const initialCache = new Map<string, string>();
const isHan = (ch: string) => /[\u3400-\u9fff]/.test(ch);

export type MatchRange = [start: number, end: number];

/** 单个字符的拼音首字母（非汉字返回自身小写） */
export function pinyinInitial(ch: string): string {
  if (!isHan(ch)) return ch.toLowerCase();
  const cached = initialCache.get(ch);
  if (cached !== undefined) return cached;
  let letter = "";
  for (let i = 0; i < PINYIN_BOUNDARY.length; i += 1) {
    if (collator.compare(PINYIN_BOUNDARY[i], ch) <= 0) letter = PINYIN_LETTERS[i];
    else break;
  }
  const result = letter.toLowerCase();
  initialCache.set(ch, result);
  return result;
}

// 把文本拆成「每个字符可匹配的音节」：汉字 → 全拼（若提供）或首字母；空白 → ""；其他 → 自身小写
function toUnits(text: string, pinyin?: string): string[] {
  const syllables = (pinyin ?? "").trim().toLowerCase().split(/\s+/).filter(Boolean);
  let hanIndex = 0;
  return [...text].map((ch) => {
    if (/\s/.test(ch)) return "";
    if (!isHan(ch)) return ch.toLowerCase();
    const full = syllables[hanIndex];
    hanIndex += 1;
    return full ?? pinyinInitial(ch);
  });
}

/**
 * 在 text 中查找 query，返回命中的字符区间 [start, end)（按 Unicode 字符计）或 null。
 * 1) 普通子串（忽略大小写）
 * 2) 拼音：从任意字符开始，每个连续字符消耗其音节的一个非空前缀，直到 query 被完全消耗。
 *    因此 cs / ceshi / ces / cesh 都能命中「测试」。
 */
export function matchText(text: string, query: string, pinyin?: string): MatchRange | null {
  const trimmed = query.trim();
  const q = trimmed.toLowerCase().replace(/\s+/g, "");
  if (!q) return null;
  const plainIndex = text.toLowerCase().indexOf(trimmed.toLowerCase());
  if (plainIndex >= 0) {
    const start = [...text.slice(0, plainIndex)].length;
    return [start, start + [...trimmed].length];
  }
  const units = toUnits(text, pinyin);
  const consume = (k: number, qi: number): number => {
    if (qi === q.length) return k;
    if (k >= units.length) return -1;
    const unit = units[k];
    if (unit === "") return consume(k + 1, qi);
    for (let len = Math.min(unit.length, q.length - qi); len >= 1; len -= 1) {
      if (unit.slice(0, len) === q.slice(qi, qi + len)) {
        const end = consume(k + 1, qi + len);
        if (end >= 0) return end;
      }
    }
    return -1;
  };
  for (let start = 0; start < units.length; start += 1) {
    if (units[start] === "") continue;
    const end = consume(start, 0);
    if (end >= 0) return [start, end];
  }
  return null;
}

/** 按命中区间把文本切成三段，便于模板高亮 */
export function splitByRange(text: string, range: MatchRange | null): [string, string, string] {
  const chars = [...text];
  if (!range) return [text, "", ""];
  return [chars.slice(0, range[0]).join(""), chars.slice(range[0], range[1]).join(""), chars.slice(range[1]).join("")];
}
