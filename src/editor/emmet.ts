const CSS_MAP: Record<string, string> = {
  d: "display",
  m: "margin",
  p: "padding",
  w: "width",
  h: "height",
  t: "top",
  b: "bottom",
  l: "left",
  r: "right",
  c: "color",
  bg: "background",
  bd: "border",
  fz: "font-size",
  fw: "font-weight",
  ta: "text-align",
  pos: "position",
  ai: "align-items",
  jc: "justify-content",
  fd: "flex-direction",
};

const CSS_VALUES: Record<string, string> = {
  f: "flex",
  b: "block",
  n: "none",
  rel: "relative",
  abs: "absolute",
  c: "center",
  "0": "0",
};

export function expandEmmet(abbreviation: string, language: string): string | null {
  const abbr = abbreviation.trim();
  if (!abbr) return null;
  if (language === "css" || language === "scss" || language === "less") {
    return expandCss(abbr);
  }
  if (language === "html" || language === "handlebars" || language === "razor" || language === "markdown") {
    return expandHtml(abbr);
  }
  return null;
}

export function emmetTrigger(textUntilPosition: string): string | null {
  const match = textUntilPosition.match(/[\w.#*\->+[\]=]+$/);
  return match?.[0] ?? null;
}

function expandCss(abbr: string): string | null {
  const parts = abbr.split("-");
  const prop = CSS_MAP[parts[0]] ?? (parts[0].length > 1 ? parts[0] : null);
  if (!prop) return null;
  const value = parts
    .slice(1)
    .map((part) => {
      if (CSS_VALUES[part]) return CSS_VALUES[part];
      if (/^\d+$/.test(part)) return `${part}px`;
      return part;
    })
    .join(" ");
  if (!value) return `${prop}: $0;`;
  return `${prop}: ${value};`;
}

function expandHtml(abbr: string): string | null {
  if (!/^[a-zA-Z.#[]/.test(abbr) || /\s/.test(abbr)) return null;
  const countMatch = abbr.match(/^(.*?)\*(\d+)$/);
  const count = countMatch ? Number(countMatch[2]) : 1;
  const body = countMatch ? countMatch[1] : abbr;
  const chain = body.split(">").filter(Boolean);
  if (!chain.length || chain.length > 6) return null;
  let html = expandHtmlNode(chain[chain.length - 1], "$0");
  if (!html) return null;
  for (let i = chain.length - 2; i >= 0; i -= 1) {
    html = expandHtmlNode(chain[i], html);
    if (!html) return null;
  }
  return Array.from({ length: count }, () => html).join("\n");
}

function expandHtmlNode(token: string, inner: string): string | null {
  const match = token.match(/^([a-zA-Z][\w-]*)?(#[\w-]+)?((?:\.[\w-]+)*)$/);
  if (!match) return null;
  const tag = match[1] || "div";
  const id = match[2] ? ` id="${match[2].slice(1)}"` : "";
  const classes = (match[3].match(/\./g) || []).length
    ? ` class="${match[3].split(".").filter(Boolean).join(" ")}"`
    : "";
  if (["br", "hr", "img", "input", "meta", "link"].includes(tag)) {
    return `<${tag}${id}${classes} />`;
  }
  return `<${tag}${id}${classes}>${inner}</${tag}>`;
}
