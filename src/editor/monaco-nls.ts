import { watch } from "vue";
import { locale, t, type Locale } from "../i18n";

/**
 * Monaco 打包后 localize(index, fallback) 读 globalThis._VSCODE_NLS_MESSAGES[index]。
 * 索引随 monaco 小版本可能变动；当前对齐 monaco-editor@0.56。
 * 文案取自各语言包对同名 key 的翻译。
 */
const MONACO_NLS: Record<number, string> = {
  // diff hide unchanged
  140: "monaco.diff.foldUnchanged",
  141: "monaco.diff.hiddenLines",
  142: "monaco.diff.showMoreAbove",
  143: "monaco.diff.showUnchanged",
  144: "monaco.diff.showMoreBelow",
  145: "monaco.diff.hiddenLines",
  146: "monaco.diff.doubleClickUnfold",
  // diff actions
  128: "monaco.diff.copyDeletedLines",
  129: "monaco.diff.copyDeletedLine",
  130: "monaco.diff.copyChangedLines",
  131: "monaco.diff.copyChangedLine",
  132: "monaco.diff.copyDeletedLineN",
  133: "monaco.diff.copyChangedLineN",
  134: "monaco.diff.revertThisChange",
  151: "monaco.diff.revertSelectedChanges",
  152: "monaco.diff.revertChange",
  // find
  1003: "monaco.find.dialogLabel",
  1004: "monaco.find.find",
  1005: "monaco.find.find",
  1006: "monaco.find.previousMatch",
  1007: "monaco.find.nextMatch",
  1008: "monaco.find.findInSelection",
  1009: "monaco.find.close",
  1010: "monaco.find.replace",
  1011: "monaco.find.replace",
  1012: "monaco.find.replace",
  1013: "monaco.find.replaceAll",
  1014: "monaco.find.toggleReplace",
  1016: "monaco.find.matchesLocation",
  1017: "monaco.find.noResults",
  // clipboard / edit
  74: "monaco.edit.undo",
  75: "monaco.edit.undo",
  77: "monaco.edit.redo",
  78: "monaco.edit.redo",
  80: "monaco.edit.selectAll",
  81: "monaco.edit.selectAll",
  850: "monaco.edit.cut",
  851: "monaco.edit.cut",
  852: "monaco.edit.cut",
  854: "monaco.edit.copy",
  855: "monaco.edit.copy",
  856: "monaco.edit.copy",
  858: "monaco.edit.paste",
  859: "monaco.edit.paste",
  860: "monaco.edit.paste",
  1171: "monaco.edit.copy",
  // readonly tip
  1427: "monaco.editor.readonlyInput",
  1428: "monaco.editor.readonly",
  // goto / peek
  1106: "monaco.goto.noResultsFor",
  1128: "monaco.goto.noResults",
  1134: "monaco.goto.noResultsFound",
};

declare global {
  // eslint-disable-next-line no-var
  var _VSCODE_NLS_MESSAGES: string[] | undefined;
  // eslint-disable-next-line no-var
  var _VSCODE_NLS_LANGUAGE: string | undefined;
}

let watching = false;

export function monacoReadOnlyMessage() {
  return { value: t("monaco.editor.readonly") };
}

export function applyMonacoNls(language: Locale = locale.value) {
  const messages = globalThis._VSCODE_NLS_MESSAGES ? [...globalThis._VSCODE_NLS_MESSAGES] : [];
  for (const [index, key] of Object.entries(MONACO_NLS)) {
    messages[Number(index)] = t(key);
  }
  globalThis._VSCODE_NLS_MESSAGES = messages;
  globalThis._VSCODE_NLS_LANGUAGE = language;
}

export function watchMonacoNls() {
  if (watching) return;
  watching = true;
  applyMonacoNls();
  watch(locale, (next) => applyMonacoNls(next));
}
