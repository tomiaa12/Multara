declare module "monaco-editor/editor/browser/widget/diffEditor/features/hideUnchangedRegionsFeature.js" {
  import type { editor, languages } from "monaco-editor";

  export type DiffEditorBreadcrumbItem = {
    name: string;
    kind: languages.SymbolKind;
    startLineNumber: number;
  };

  export interface IDiffEditorBreadcrumbsSource {
    dispose(): void;
    getBreadcrumbItems(
      startRange: {
        startLineNumber: number;
        endLineNumberExclusive: number;
        contains?(lineNumber: number): boolean;
      },
      reader?: unknown,
    ): DiffEditorBreadcrumbItem[];
    getAt(lineNumber: number, reader?: unknown): DiffEditorBreadcrumbItem[];
  }

  export class HideUnchangedRegionsFeature {
    static setBreadcrumbsSourceFactory(
      factory: (
        textModel: editor.ITextModel,
        instantiationService: unknown,
      ) => IDiffEditorBreadcrumbsSource,
    ): void;
  }
}
