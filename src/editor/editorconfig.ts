export type EditorConfigProps = {
  indentStyle?: "tab" | "space";
  indentSize?: number;
  tabWidth?: number;
  endOfLine?: "lf" | "crlf" | "cr";
  insertFinalNewline?: boolean;
};

export type EditorConfigSection = {
  glob: string;
  root?: boolean;
  props: EditorConfigProps;
};

export function editorConfigSearchPaths(filePath: string): string[] {
  const parts = filePath.replace(/\\/g, "/").split("/").filter(Boolean).slice(0, -1);
  const paths = [".editorconfig"];
  let acc = "";
  for (const part of parts) {
    acc = acc ? `${acc}/${part}` : part;
    paths.push(`${acc}/.editorconfig`);
  }
  return paths;
}

export function parseEditorConfig(text: string): EditorConfigSection[] {
  const sections: EditorConfigSection[] = [];
  let current: EditorConfigSection | null = null;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/[#;].*$/, "").trim();
    if (!line) continue;
    const header = line.match(/^\[(.+)]$/);
    if (header) {
      current = { glob: header[1], props: {} };
      sections.push(current);
      continue;
    }
    const eq = line.indexOf("=");
    if (eq < 0) continue;
    const key = line.slice(0, eq).trim().toLowerCase();
    const value = line.slice(eq + 1).trim().toLowerCase();
    if (key === "root" && !current) {
      continue;
    }
    if (key === "root" && current) {
      current.root = value === "true";
      continue;
    }
    if (!current) {
      current = { glob: "*", props: {} };
      sections.push(current);
    }
    if (key === "indent_style" && (value === "tab" || value === "space")) {
      current.props.indentStyle = value;
    }
    if (key === "indent_size") {
      const size = Number(value);
      if (Number.isFinite(size) && size > 0) current.props.indentSize = size;
    }
    if (key === "tab_width") {
      const size = Number(value);
      if (Number.isFinite(size) && size > 0) current.props.tabWidth = size;
    }
    if (key === "end_of_line" && (value === "lf" || value === "crlf" || value === "cr")) {
      current.props.endOfLine = value;
    }
    if (key === "insert_final_newline") {
      current.props.insertFinalNewline = value === "true";
    }
  }
  return sections;
}

export function matchEditorConfig(
  filePath: string,
  sections: EditorConfigSection[],
): EditorConfigProps {
  const name = filePath.replace(/\\/g, "/");
  const merged: EditorConfigProps = {};
  for (const section of sections) {
    if (globMatches(section.glob, name)) Object.assign(merged, section.props);
  }
  return merged;
}

export function editorOptionsFromConfig(props: EditorConfigProps): {
  insertSpaces?: boolean;
  tabSize?: number;
  eol?: "LF" | "CRLF";
} {
  const tabSize = props.indentSize ?? props.tabWidth;
  return {
    insertSpaces: props.indentStyle === "tab" ? false : props.indentStyle === "space" ? true : undefined,
    tabSize,
    eol: props.endOfLine === "crlf" ? "CRLF" : props.endOfLine === "lf" ? "LF" : undefined,
  };
}

function globMatches(glob: string, filePath: string): boolean {
  const name = filePath.split("/").pop() ?? filePath;
  const pattern = glob
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*\*/g, "{{GLOBSTAR}}")
    .replace(/\*/g, "[^/]*")
    .replace(/\?/g, "[^/]")
    .replace(/{{GLOBSTAR}}/g, ".*");
  const re = new RegExp(`^${pattern}$`, "i");
  return re.test(name) || re.test(filePath);
}
