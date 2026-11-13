/** 文件树和 Git 树共用的缩进。文件图标靠同一列，是因为每一行都占一个箭头的宽度。 */
export const TREE_ROW_INSET = 18;
export const TREE_INDENT = 14;
export const TREE_CHEVRON = 16;

export function treeRowPad(depth: number) {
  return TREE_ROW_INSET + depth * TREE_INDENT;
}

export function treeGuideLeft(level: number) {
  return TREE_ROW_INSET + (level - 1) * TREE_INDENT + TREE_CHEVRON / 2;
}
