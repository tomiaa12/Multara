import { shallowRef } from "vue";

/** 页面写入的标题栏内容；为 null 时标题栏回退显示路由 meta.title。 */
export interface TitlebarDetail {
  title: string;
  summary?: string;
}

export const titlebarDetail = shallowRef<TitlebarDetail | null>(null);
