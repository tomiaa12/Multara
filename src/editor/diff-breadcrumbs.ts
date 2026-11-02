import type * as Monaco from "monaco-editor";
import { languages } from "monaco-editor";
import {
  HideUnchangedRegionsFeature,
  type IDiffEditorBreadcrumbsSource,
} from "monaco-editor/editor/browser/widget/diffEditor/features/hideUnchangedRegionsFeature.js";

type OutlineItem = {
  name: string;
  kind: languages.SymbolKind;
  startLineNumber: number;
  endLineNumber: number;
};

type LineRangeLike = {
  startLineNumber: number;
  endLineNumberExclusive: number;
  contains?(lineNumber: number): boolean;
};

const OUTLINE_PATTERNS: { re: RegExp; kind: languages.SymbolKind }[] = [
  { re: /^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+([A-Za-z_]\w*)/, kind: languages.SymbolKind.Function },
  { re: /^\s*(?:export\s+)?(?:default\s+)?class\s+([A-Za-z_]\w*)/, kind: languages.SymbolKind.Class },
  {
    re: /^\s*(?:export\s+)?(?:async\s+)?(?:const|let|var)\s+([A-Za-z_]\w*)\s*=\s*(?:async\s*)?(?:\(|function\b)/,
    kind: languages.SymbolKind.Function,
  },
  { re: /^\s*(?:export\s+)?(?:interface|type|enum)\s+([A-Za-z_]\w*)/, kind: languages.SymbolKind.Interface },
  { re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)/, kind: languages.SymbolKind.Function },
  {
    re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|impl(?:\s*<[^>]*>)?(?:\s+[A-Za-z_]\w*)?)\s+([A-Za-z_]\w*)/,
    kind: languages.SymbolKind.Class,
  },
  { re: /^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)/, kind: languages.SymbolKind.Function },
  { re: /^\s*class\s+([A-Za-z_]\w*)/, kind: languages.SymbolKind.Class },
];

function lineContains(range: LineRangeLike, lineNumber: number) {
  if (typeof range.contains === "function") return range.contains(lineNumber);
  return lineNumber >= range.startLineNumber && lineNumber < range.endLineNumberExclusive;
}

function buildOutline(model: Monaco.editor.ITextModel): OutlineItem[] {
  const starts: { name: string; kind: languages.SymbolKind; startLineNumber: number; indent: number }[] = [];
  const lineCount = model.getLineCount();
  for (let line = 1; line <= lineCount; line += 1) {
    const text = model.getLineContent(line);
    const indent = text.match(/^\s*/)?.[0]?.length ?? 0;
    for (const pattern of OUTLINE_PATTERNS) {
      const match = text.match(pattern.re);
      if (!match?.[1]) continue;
      starts.push({ name: match[1], kind: pattern.kind, startLineNumber: line, indent });
      break;
    }
  }
  return starts.map((item, index) => {
    let endLineNumber = lineCount;
    for (let next = index + 1; next < starts.length; next += 1) {
      if (starts[next].indent > item.indent) continue;
      endLineNumber = starts[next].startLineNumber - 1;
      break;
    }
    return {
      name: item.name,
      kind: item.kind,
      startLineNumber: item.startLineNumber,
      endLineNumber: Math.max(item.startLineNumber, endLineNumber),
    };
  });
}

function sortBySpan(items: OutlineItem[]) {
  return [...items].sort(
    (a, b) => b.endLineNumber - b.startLineNumber - (a.endLineNumber - a.startLineNumber),
  );
}

let registered = false;

/** 隐藏未改区域时显示可点击的符号路径。 */
export function registerDiffBreadcrumbs() {
  if (registered) return;
  registered = true;
  HideUnchangedRegionsFeature.setBreadcrumbsSourceFactory((textModel): IDiffEditorBreadcrumbsSource => {
    let outline = buildOutline(textModel);
    const disposable = textModel.onDidChangeContent(() => {
      outline = buildOutline(textModel);
    });
    return {
      dispose: () => disposable.dispose(),
      getBreadcrumbItems(startRange: LineRangeLike) {
        return sortBySpan(
          outline.filter(
            (item) =>
              lineContains(startRange, item.startLineNumber) &&
              !lineContains(startRange, item.endLineNumber),
          ),
        ).map((item) => ({
          name: item.name,
          kind: item.kind,
          startLineNumber: item.startLineNumber,
        }));
      },
      getAt(lineNumber: number) {
        return sortBySpan(
          outline.filter(
            (item) => item.startLineNumber <= lineNumber && lineNumber <= item.endLineNumber,
          ),
        ).map((item) => ({
          name: item.name,
          kind: item.kind,
          startLineNumber: item.startLineNumber,
        }));
      },
    };
  });
}
