export type EditorEol = "LF" | "CRLF";

export function detectEol(content: string): EditorEol {
  return content.includes("\r\n") ? "CRLF" : "LF";
}

export function convertEol(content: string, eol: EditorEol): string {
  const normalized = content.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  return eol === "CRLF" ? normalized.replace(/\n/g, "\r\n") : normalized;
}

export function indentLabel(insertSpaces: boolean, tabSize: number): string {
  return insertSpaces ? `空格: ${tabSize}` : `Tab 大小: ${tabSize}`;
}

export function detectIndent(content: string): { insertSpaces: boolean; tabSize: number } {
  const lines = content.split(/\r?\n/).slice(0, 80);
  let tabs = 0;
  let spaces2 = 0;
  let spaces4 = 0;
  for (const line of lines) {
    if (line.startsWith("\t")) tabs += 1;
    else if (/^ {4}/.test(line)) spaces4 += 1;
    else if (/^ {2}/.test(line)) spaces2 += 1;
  }
  if (tabs > spaces2 && tabs > spaces4) return { insertSpaces: false, tabSize: 4 };
  if (spaces2 > spaces4) return { insertSpaces: true, tabSize: 2 };
  return { insertSpaces: true, tabSize: 4 };
}
