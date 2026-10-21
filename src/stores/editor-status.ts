import { computed, ref } from "vue";
import { defineStore } from "pinia";
import {
  AUTO_DETECT_LANGUAGE,
  applyLanguageOverride,
  editorLanguageKey,
  languageDisplayName,
  type EditorLanguageOption,
} from "../monaco-language";
import type { FileContent } from "../domain/types";
import { indentLabel, type EditorEol } from "../editor/eol";
import type { OutlineItem } from "../editor/outline";

const OVERRIDE_KEY = "multara.editor.languageOverrides";

function loadOverrides(): Record<string, string> {
  try {
    const raw = localStorage.getItem(OVERRIDE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object") return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(
        (entry): entry is [string, string] => typeof entry[1] === "string",
      ),
    );
  } catch {
    return {};
  }
}

export const useEditorStatusStore = defineStore("editor-status", () => {
  const fileKey = ref<string | null>(null);
  const languageId = ref<string | null>(null);
  const languages = ref<EditorLanguageOption[]>([]);
  const overrides = ref<Record<string, string>>(loadOverrides());
  const selection = ref<{
    key: string;
    language: string;
    nonce: number;
  } | null>(null);
  const line = ref(1);
  const column = ref(1);
  const eol = ref<EditorEol>("LF");
  const insertSpaces = ref(true);
  const tabSize = ref(2);
  const encoding = ref("UTF-8");
  const blameLabel = ref("");
  const blameTitle = ref("");
  const outline = ref<OutlineItem[]>([]);
  const documentAction = ref<{
    type: "eol" | "indent" | "goto" | "reveal";
    eol?: EditorEol;
    insertSpaces?: boolean;
    tabSize?: number;
    line?: number;
    nonce: number;
  } | null>(null);

  const languageLabel = computed(() =>
    languageId.value ? languageDisplayName(languageId.value, languages.value) : "",
  );
  const hasOverride = computed(() =>
    Boolean(fileKey.value && overrides.value[fileKey.value]),
  );
  const positionLabel = computed(() => `Ln ${line.value}, Col ${column.value}`);
  const indentText = computed(() => indentLabel(insertSpaces.value, tabSize.value));

  function persistOverrides() {
    localStorage.setItem(OVERRIDE_KEY, JSON.stringify(overrides.value));
  }

  function setLanguages(items: EditorLanguageOption[]) {
    languages.value = items;
  }

  function syncActive(key: string | null, language: string | null) {
    fileKey.value = key;
    languageId.value = language;
    if (!key) {
      outline.value = [];
    }
  }

  function syncDocument(state: {
    line: number;
    column: number;
    eol: EditorEol;
    insertSpaces: boolean;
    tabSize: number;
    outline: OutlineItem[];
  } | null) {
    if (!state) {
      line.value = 1;
      column.value = 1;
      eol.value = "LF";
      insertSpaces.value = true;
      tabSize.value = 2;
      outline.value = [];
      blameLabel.value = "";
      blameTitle.value = "";
      return;
    }
    const outlineSame =
      outline.value.length === state.outline.length &&
      outline.value.every((item, index) => item.line === state.outline[index]?.line && item.label === state.outline[index]?.label);
    if (
      line.value === state.line &&
      column.value === state.column &&
      eol.value === state.eol &&
      insertSpaces.value === state.insertSpaces &&
      tabSize.value === state.tabSize &&
      outlineSame
    ) {
      return;
    }
    line.value = state.line;
    column.value = state.column;
    eol.value = state.eol;
    insertSpaces.value = state.insertSpaces;
    tabSize.value = state.tabSize;
    outline.value = state.outline;
  }

  function setBlame(label: string, title = "") {
    blameLabel.value = label;
    blameTitle.value = title;
  }

  function requestDocument(action: Omit<NonNullable<typeof documentAction.value>, "nonce">) {
    documentAction.value = { ...action, nonce: Date.now() };
  }

  function applyOverride(file: FileContent): FileContent {
    return applyLanguageOverride(file, overrides.value);
  }

  function choose(language: string) {
    if (!fileKey.value) return;
    if (language === AUTO_DETECT_LANGUAGE) {
      const next = { ...overrides.value };
      delete next[fileKey.value];
      overrides.value = next;
    } else {
      overrides.value = { ...overrides.value, [fileKey.value]: language };
      languageId.value = language;
    }
    persistOverrides();
    selection.value = { key: fileKey.value, language, nonce: Date.now() };
  }

  function renameOverride(fromKey: string, toKey: string) {
    const language = overrides.value[fromKey];
    if (!language) return;
    const next = { ...overrides.value };
    delete next[fromKey];
    next[toKey] = language;
    overrides.value = next;
    persistOverrides();
    if (fileKey.value === fromKey) fileKey.value = toKey;
  }

  return {
    fileKey,
    languageId,
    languageLabel,
    languages,
    hasOverride,
    selection,
    line,
    column,
    eol,
    insertSpaces,
    tabSize,
    encoding,
    blameLabel,
    blameTitle,
    outline,
    positionLabel,
    indentText,
    documentAction,
    setLanguages,
    syncActive,
    syncDocument,
    setBlame,
    requestDocument,
    applyOverride,
    choose,
    renameOverride,
  };
});
