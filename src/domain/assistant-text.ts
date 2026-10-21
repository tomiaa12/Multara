export function applyAssistantChunk(
  current: string,
  chunk: string,
  options: { replace?: boolean; completed?: boolean } = {},
): string {
  if (options.completed || options.replace) return chunk;
  return current ? `${current}${chunk}` : chunk;
}

export function liveAssistantText(
  items: Array<{ eventType: string; content?: string | null; replacesFinalText?: boolean }>,
): string {
  const texts = items.filter((item) => item.eventType === "text" && item.content);
  if (!texts.length) return "";
  let start = 0;
  for (let index = texts.length - 1; index >= 0; index -= 1) {
    if (texts[index]?.replacesFinalText) {
      start = index;
      break;
    }
  }
  return texts
    .slice(start)
    .map((item) => item.content ?? "")
    .join("");
}
