/**
 * `v-model.number` 清空时得到 ""，输入非法时可能是 NaN；这类值不提交，保留上次保存的设置。
 * 合法时返回 `{ [key]: "整数" }`，便于直接展开进设置载荷。
 */
export function integerSettingEntry(key: string, value: unknown, min: number, max: number): Record<string, string> {
  if (value === "" || value == null) return {};
  const number = Number(value);
  if (!Number.isInteger(number) || number < min || number > max) return {};
  return { [key]: String(number) };
}
