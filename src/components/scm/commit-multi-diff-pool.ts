/**
 * 少量 DiffEditor 模板轮换绑定可见文件，
 * 滚出视口即 release（清 model、归还池），避免 N 文件 = N 个 DiffEditor。
 */
import { registerDiffBreadcrumbs } from "../../editor/diff-breadcrumbs";
import { monacoReadOnlyMessage } from "../../editor/monaco-nls";
import { monacoOverflowWidgetsRoot } from "../../editor/monaco-overflow-widgets";
import { settleDiffEditor } from "../../editor/monaco-settle";
import { applyMonacoTheme } from "../../editor/monaco-workspace";
import { languageFromFilePath } from "../../monaco-language";
import { monaco, monacoLanguageId } from "../../monaco";

export type MultiDiffPoolFile = {
  diffId: string;
  path: string;
  original: string;
  modified: string;
  language: string;
  binary: boolean;
};

const SIDE_BY_SIDE_BREAKPOINT = 560;
const AUTO_HEIGHT_MIN = 80;

function nextFrame() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

export class CommitMultiDiffPool {
  private readonly free: Template[] = [];
  private readonly busy = new Map<string, Template>();
  private readonly garage: HTMLElement;
  private theme: string;
  private disposed = false;

  constructor(
    theme: string,
    private readonly onContentHeight: (path: string, height: number) => void,
  ) {
    this.theme = theme;
    this.garage = document.createElement("div");
    this.garage.className = "commit-multi-diff-pool-garage";
    this.garage.setAttribute("aria-hidden", "true");
    document.body.appendChild(this.garage);
  }

  setTheme(theme: string) {
    this.theme = theme;
    monaco.editor.setTheme(theme);
  }

  /** 把模板绑到 body；同 path 重复 bind 会先 release。 */
  bind(body: HTMLElement, file: MultiDiffPoolFile) {
    if (this.disposed) return;
    const existing = this.busy.get(file.path);
    if (existing) {
      if (existing.body === body && existing.fileKey === fileKey(file)) {
        existing.layout();
        return;
      }
      this.release(file.path);
    }
    if (file.binary) {
      body.replaceChildren();
      const tip = document.createElement("p");
      tip.className = "git-diff-binary";
      tip.textContent = "二进制文件，无法比较内容。";
      body.appendChild(tip);
      this.onContentHeight(file.path, 48);
      return;
    }
    const template = this.free.pop() ?? new Template();
    template.attach(body, file);
    this.busy.set(file.path, template);
    void template.load(file, this.theme, (height) => {
      if (this.disposed || this.busy.get(file.path) !== template) return;
      this.onContentHeight(file.path, height);
    });
  }

  release(path: string) {
    const template = this.busy.get(path);
    if (!template) return;
    this.busy.delete(path);
    template.detach(this.garage);
    this.free.push(template);
  }

  releaseAll() {
    for (const path of [...this.busy.keys()]) this.release(path);
  }

  dispose() {
    this.disposed = true;
    this.releaseAll();
    for (const template of this.free) template.dispose();
    this.free.length = 0;
    this.garage.remove();
  }
}

function fileKey(file: MultiDiffPoolFile) {
  return `${file.diffId}\0${file.path}\0${file.original.length}\0${file.modified.length}\0${file.language}`;
}

function modelUri(diffId: string, path: string, side: "original" | "modified") {
  const file = path.replace(/^\/+/, "") || "file.txt";
  return monaco.Uri.from({
    scheme: "scm-diff",
    path: `/${side}/${diffId}/${file}`,
  });
}

function resolveLanguage(language: string, path: string) {
  const fromProp = language?.trim();
  const candidate = fromProp && fromProp !== "plaintext" ? fromProp : languageFromFilePath(path);
  return monacoLanguageId(candidate);
}

class Template {
  readonly host: HTMLElement;
  body: HTMLElement | null = null;
  fileKey = "";
  private editor: monaco.editor.IStandaloneDiffEditor | null = null;
  private originalModel: monaco.editor.ITextModel | null = null;
  private modifiedModel: monaco.editor.ITextModel | null = null;
  private sizeDisposable: { dispose: () => void } | null = null;
  private loadToken = 0;
  private boundPath = "";
  /** diff 未就绪前不回写高度 */
  private heightReady = false;

  constructor() {
    this.host = document.createElement("div");
    this.host.className = "monaco-host git-diff-editor is-diff-pending";
    this.host.style.visibility = "hidden";
  }

  attach(body: HTMLElement, file: MultiDiffPoolFile) {
    this.body = body;
    this.boundPath = file.path;
    this.fileKey = fileKey(file);
    body.replaceChildren();
    body.appendChild(this.host);
  }

  detach(garage: HTMLElement) {
    this.loadToken += 1;
    this.heightReady = false;
    this.clearModels();
    this.host.classList.add("is-diff-pending");
    this.host.style.visibility = "hidden";
    this.host.style.height = "";
    garage.appendChild(this.host);
    this.body = null;
    this.boundPath = "";
    this.fileKey = "";
  }

  layout() {
    if (!this.editor || !this.body || !this.heightReady) return;
    const height = Math.max(Number.parseFloat(this.host.style.height) || 0, AUTO_HEIGHT_MIN);
    this.editor.layout({ width: this.body.clientWidth, height });
  }

  async load(
    file: MultiDiffPoolFile,
    theme: string,
    onHeight: (height: number) => void,
  ) {
    const token = ++this.loadToken;
    this.heightReady = false;
    this.host.classList.add("is-diff-pending");
    this.host.style.visibility = "hidden";
    registerDiffBreadcrumbs();
    applyMonacoTheme();
    if (!this.editor) {
      this.editor = monaco.editor.createDiffEditor(this.host, {
        readOnly: true,
        readOnlyMessage: monacoReadOnlyMessage(),
        originalEditable: false,
        automaticLayout: false,
        renderSideBySide: true,
        useInlineViewWhenSpaceIsLimited: true,
        renderSideBySideInlineBreakpoint: SIDE_BY_SIDE_BREAKPOINT,
        enableSplitViewResizing: true,
        renderMarginRevertIcon: false,
        renderGutterMenu: false,
        renderIndicators: true,
        renderOverviewRuler: false,
        ignoreTrimWhitespace: true,
        hideUnchangedRegions: {
          enabled: true,
          contextLineCount: 3,
          minimumLineCount: 3,
          revealLineCount: 20,
        },
        theme,
        fontSize: 13,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        scrollbar: {
          vertical: "hidden",
          horizontal: "hidden",
          handleMouseWheel: false,
          alwaysConsumeMouseWheel: false,
          useShadows: false,
        },
        overflowWidgetsDomNode: monacoOverflowWidgetsRoot(),
      });
      const semantic = { "semanticHighlighting.enabled": true as const };
      this.editor.getOriginalEditor().updateOptions(semantic);
      this.editor.getModifiedEditor().updateOptions(semantic);
    } else {
      monaco.editor.setTheme(theme);
    }

    this.clearModels();
    const language = resolveLanguage(file.language, file.path);
    const originalUri = modelUri(file.diffId, file.path, "original");
    const modifiedUri = modelUri(file.diffId, file.path, "modified");
    monaco.editor.getModel(originalUri)?.dispose();
    monaco.editor.getModel(modifiedUri)?.dispose();
    this.originalModel = monaco.editor.createModel(file.original, language, originalUri);
    this.modifiedModel = monaco.editor.createModel(file.modified, language, modifiedUri);

    // 先 createViewModel + waitForDiff，折叠区算完再 setModel，避免「全文 → N 隐藏行」
    const viewModel = this.editor.createViewModel({
      original: this.originalModel,
      modified: this.modifiedModel,
    });
    try {
      await viewModel.waitForDiff();
    } catch {
      viewModel.dispose();
      return;
    }
    if (token !== this.loadToken) {
      viewModel.dispose();
      return;
    }
    this.editor.setModel(viewModel);
    // hideUnchangedRegions 的 view zone 在 autorun 里应用，再等两帧再量高度/露脸
    await nextFrame();
    if (token !== this.loadToken || !this.body) return;
    await settleDiffEditor(this.editor, this.body, () => token === this.loadToken);
    if (token !== this.loadToken) return;
    const languageNow = resolveLanguage(file.language, file.path);
    const originalModel = this.originalModel;
    const modifiedModel = this.modifiedModel;
    if (originalModel && originalModel.getLanguageId() !== languageNow) {
      monaco.editor.setModelLanguage(originalModel, languageNow);
    }
    if (modifiedModel && modifiedModel.getLanguageId() !== languageNow) {
      monaco.editor.setModelLanguage(modifiedModel, languageNow);
    }

    const sync = () => {
      if (!this.editor || !this.body || token !== this.loadToken || !this.heightReady) return;
      const height = Math.max(
        this.editor.getOriginalEditor().getContentHeight(),
        this.editor.getModifiedEditor().getContentHeight(),
        AUTO_HEIGHT_MIN,
      );
      this.host.style.height = `${height}px`;
      this.editor.layout({ width: this.body.clientWidth, height });
      onHeight(height);
    };

    this.sizeDisposable?.dispose();
    const original = this.editor.getOriginalEditor();
    const modified = this.editor.getModifiedEditor();
    const a = original.onDidContentSizeChange(sync);
    const b = modified.onDidContentSizeChange(sync);
    this.sizeDisposable = {
      dispose() {
        a.dispose();
        b.dispose();
      },
    };

    this.heightReady = true;
    sync();
    this.host.classList.remove("is-diff-pending");
    this.host.style.visibility = "";
    sync();
  }

  dispose() {
    this.loadToken += 1;
    this.clearModels();
    this.editor?.dispose();
    this.editor = null;
    this.host.remove();
  }

  private clearModels() {
    this.sizeDisposable?.dispose();
    this.sizeDisposable = null;
    this.editor?.setModel(null);
    this.originalModel?.dispose();
    this.modifiedModel?.dispose();
    this.originalModel = null;
    this.modifiedModel = null;
  }
}
