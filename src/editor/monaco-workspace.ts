import * as monaco from "monaco-editor";
import { typescript } from "monaco-editor";
import { emmetTrigger, expandEmmet } from "./emmet";
import type { WorkspaceCompilerOptions } from "./tsconfig-options";

const JSX: Record<string, typescript.JsxEmit> = {
  none: typescript.JsxEmit.None,
  preserve: typescript.JsxEmit.Preserve,
  react: typescript.JsxEmit.React,
  "react-native": typescript.JsxEmit.ReactNative,
  "react-jsx": typescript.JsxEmit.ReactJSX,
  "react-jsxdev": typescript.JsxEmit.ReactJSXDev,
};

const TARGET: Record<string, typescript.ScriptTarget> = {
  es3: typescript.ScriptTarget.ES3,
  es5: typescript.ScriptTarget.ES5,
  es6: typescript.ScriptTarget.ES2015,
  es2015: typescript.ScriptTarget.ES2015,
  es2016: typescript.ScriptTarget.ES2016,
  es2017: typescript.ScriptTarget.ES2017,
  es2018: typescript.ScriptTarget.ES2018,
  es2019: typescript.ScriptTarget.ES2019,
  es2020: typescript.ScriptTarget.ES2020,
  esnext: typescript.ScriptTarget.ESNext,
};

export function monacoThemeName(theme: string | undefined): "vs" | "vs-dark" {
  return theme === "light" ? "vs" : "vs-dark";
}

export function currentDocumentTheme(): "light" | "dark" {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

export function applyMonacoTheme(theme = currentDocumentTheme()) {
  monaco.editor.setTheme(monacoThemeName(theme));
}

export function watchDocumentTheme(onChange?: (theme: "light" | "dark") => void) {
  const notify = () => {
    const theme = currentDocumentTheme();
    applyMonacoTheme(theme);
    onChange?.(theme);
  };
  notify();
  const observer = new MutationObserver(notify);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  return () => observer.disconnect();
}

export function applyWorkspaceTypescript(options: WorkspaceCompilerOptions) {
  const next = {
    ...typescript.typescriptDefaults.getCompilerOptions(),
    allowNonTsExtensions: true,
    allowJs: options.allowJs ?? true,
    allowSyntheticDefaultImports: options.allowSyntheticDefaultImports ?? true,
    esModuleInterop: options.esModuleInterop ?? true,
    experimentalDecorators: options.experimentalDecorators,
    isolatedModules: options.isolatedModules,
    // 工作区类型声明按 node_modules 布局注册为 extraLib，只有 Node 解析会去那里找。
    moduleResolution: typescript.ModuleResolutionKind.NodeJs,
    strict: options.strict,
    useDefineForClassFields: options.useDefineForClassFields,
    baseUrl: options.baseUrl,
    paths: options.paths,
  };
  if (options.jsx) next.jsx = JSX[options.jsx] ?? next.jsx;
  if (options.target && TARGET[options.target.toLowerCase()]) {
    next.target = TARGET[options.target.toLowerCase()];
  }
  typescript.typescriptDefaults.setCompilerOptions(next);
  typescript.javascriptDefaults.setCompilerOptions(next);
  typescript.typescriptDefaults.setEagerModelSync(true);
  typescript.javascriptDefaults.setEagerModelSync(true);
}

let emmetRegistered = false;

export function registerEmmetCompletions() {
  if (emmetRegistered) return;
  emmetRegistered = true;
  const provider: monaco.languages.CompletionItemProvider = {
    triggerCharacters: [".", "#", ">", "*", "-"],
    provideCompletionItems(model, position) {
      const language = model.getLanguageId();
      const text = model.getValueInRange({
        startLineNumber: position.lineNumber,
        startColumn: 1,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      });
      const trigger = emmetTrigger(text);
      if (!trigger) return { suggestions: [] };
      const insert = expandEmmet(trigger, language);
      if (!insert) return { suggestions: [] };
      const range = {
        startLineNumber: position.lineNumber,
        startColumn: position.column - trigger.length,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      };
      return {
        suggestions: [
          {
            label: trigger,
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: insert,
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Emmet",
            range,
          },
        ],
      };
    },
  };
  for (const language of ["html", "handlebars", "razor", "css", "scss", "less", "markdown"]) {
    monaco.languages.registerCompletionItemProvider(language, provider);
  }
}
