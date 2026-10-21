import * as monaco from "monaco-editor";
import { json } from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker.js?worker";
import CssWorker from "monaco-editor/language/css/css.worker.js?worker";
import HtmlWorker from "monaco-editor/language/html/html.worker.js?worker";
import JsonWorker from "monaco-editor/language/json/json.worker.js?worker";
import TsWorker from "monaco-editor/language/typescript/ts.worker.js?worker";
import {
  jsoncDiagnosticsOptions,
  resolveMonacoLanguageId,
  shouldSuppressEmbeddedDiagnostics,
} from "./monaco-language";
import { registerEmmetCompletions, watchDocumentTheme } from "./editor/monaco-workspace";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    if (label === "json") return new JsonWorker();
    if (label === "css" || label === "scss" || label === "less") return new CssWorker();
    if (label === "html" || label === "handlebars" || label === "razor") return new HtmlWorker();
    if (label === "typescript" || label === "javascript") return new TsWorker();
    return new EditorWorker();
  },
};

configureMonacoLanguageServices();

function configureMonacoLanguageServices() {
  json.jsonDefaults.setDiagnosticsOptions({
    ...json.jsonDefaults.diagnosticsOptions,
    ...jsoncDiagnosticsOptions,
  });

  const clearEmbeddedMarkers = (model: monaco.editor.ITextModel) => {
    const markers = monaco.editor.getModelMarkers({ resource: model.uri });
    if (!markers.length) return;
    for (const owner of new Set(markers.map((marker) => marker.owner))) {
      monaco.editor.setModelMarkers(model, owner, []);
    }
  };

  const maybeClear = (model: monaco.editor.ITextModel | null) => {
    if (model && shouldSuppressEmbeddedDiagnostics(model.getLanguageId(), model.uri.path)) {
      clearEmbeddedMarkers(model);
    }
  };

  monaco.editor.onDidCreateModel((model) => {
    maybeClear(model);
    model.onDidChangeLanguage(() => maybeClear(model));
  });

  monaco.editor.onDidChangeMarkers((uris) => {
    for (const uri of uris) {
      maybeClear(monaco.editor.getModel(uri));
    }
  });

  registerEmmetCompletions();
  watchDocumentTheme();
}

export function monacoLanguageId(language: string): string {
  return resolveMonacoLanguageId(language, (id) =>
    monaco.languages.getLanguages().some((item) => item.id === id),
  );
}

export { monaco };
export default monaco;
