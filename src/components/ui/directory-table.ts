/** 资源目录表列：表头与行共用同一套 grid track，避免每行独立布局时对不齐。 */
export type DirectoryColumn = {
  key: string;
  label?: string;
  /** CSS grid track，如 `112px`、`minmax(0, 1fr)`；禁止用裸 `auto` 做操作列 */
  size: string;
  /** 可点击表头排序 */
  sortable?: boolean;
  /** 操作列：表头左内边距与行内按钮文字对齐 */
  actions?: boolean;
};

export function directoryColumnTemplate(columns: DirectoryColumn[] | null | undefined): string {
  return (columns ?? [])
    .map((column) => column?.size?.trim())
    .filter(Boolean)
    .join(" ");
}
