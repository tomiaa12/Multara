export type SelectValue = string | number | boolean | null;

export interface SelectOption<V extends SelectValue = SelectValue> {
  value: V;
  label: string;
  /** 第二行描述文字，参与搜索 */
  description?: string;
  /** 头像：emoji / 单字 / 图片地址（data:、http(s):、/、./） */
  avatar?: string | null;
  /** 右侧附加信息 */
  meta?: string;
  disabled?: boolean;
  /** 全拼（按汉字以空格分隔，如 "ce shi"）；不提供时仅支持首字母 */
  pinyin?: string;
  /** 额外的搜索关键词 */
  keywords?: string[];
}

export interface SelectGroup<V extends SelectValue = SelectValue> {
  label: string;
  options: SelectOption<V>[];
}
