/** 文件图标主题解析出的结果：SVG 地址（img 渲染）或 Iconify 图标 id。 */
export type ResolvedFileIcon = { src: string } | { iconify: string };

export interface FileIconQuery {
  /** 只含文件名（不含目录），调用方已去掉路径前缀 */
  name: string;
  directory?: boolean;
  open?: boolean;
  light?: boolean;
}

export type FileIconResolver = (query: FileIconQuery) => ResolvedFileIcon;
