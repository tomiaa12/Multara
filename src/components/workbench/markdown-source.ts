export type MarkdownViewMode = "preview" | "source";

export function isMarkdownFile(file: { path: string; language: string }) {
  return file.language === "markdown" || file.language === "mdx" || /\.(?:md|markdown|mdx)$/i.test(file.path);
}

export function isHtmlFile(file: { path: string }) {
  return /\.html?$/i.test(file.path);
}
