import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import * as backend from "../../services/backend";
import {
  BLAME_UNCOMMITTED,
  blameEditorText,
  blameHoverText,
  blameStatusText,
  type GitBlameInfo,
} from "./git-blame";

export class GitBlame {
  private collection: Monaco.editor.IEditorDecorationsCollection | null = null;
  private timer = 0;
  private generation = 0;
  private shown: { line: number; content: string; hover: string } | null = null;
  /** 整文件 blame 缓存：移动光标只查表，不再每换一行起一次 git 子进程。 */
  private cache: { key: string; versionId: number; lines: GitBlameInfo[] | null } | null = null;
  private loading: Promise<void> | null = null;
  private readonly listeners: Array<{ dispose: () => void }> = [];

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly locate: () => { workspaceId: string; filePath: string },
    private readonly publish: (label: string, title: string) => void,
    private readonly cacheKey: () => string = () => "",
  ) {
    this.listeners.push(
      editor.onDidChangeCursorPosition(() => this.schedule()),
      editor.onDidChangeModelContent(() => this.schedule()),
      editor.onDidChangeModel(() => this.schedule()),
    );
    this.schedule();
  }

  dispose() {
    window.clearTimeout(this.timer);
    this.generation += 1;
    this.listeners.splice(0).forEach((item) => item.dispose());
    this.collection?.clear();
    this.collection = null;
    this.shown = null;
    this.cache = null;
    this.publish("", "");
  }

  /** git 视图刷新（保存、提交等）后重新取缓存。 */
  refreshSoon() {
    this.schedule();
  }

  private schedule() {
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => {
      void this.refresh();
    }, 200);
  }

  private async ensureCache(model: Monaco.editor.ITextModel) {
    const { workspaceId, filePath } = this.locate();
    const key = `${workspaceId}\0${filePath}\0${this.cacheKey()}`;
    if (this.cache?.key === key) return;
    if (this.loading) {
      await this.loading;
      if (this.cache?.key === key) return;
    }
    this.loading = (async () => {
      let lines: GitBlameInfo[] | null = null;
      try {
        lines = await backend.gitBlameFile(workspaceId, filePath);
      } catch {
        lines = null;
      }
      this.cache = { key, versionId: model.getAlternativeVersionId(), lines };
    })().finally(() => {
      this.loading = null;
    });
    await this.loading;
  }

  private async refresh() {
    const generation = ++this.generation;
    const position = this.editor.getPosition();
    const model = this.editor.getModel();
    if (!position || !model) {
      this.clear();
      return;
    }
    await this.ensureCache(model);
    if (generation !== this.generation) return;
    const cache = this.cache;
    // blame 是按磁盘内容算的；缓冲区改过之后行号对不上，宁可不显示，保存后（git 视图刷新）再出现。
    if (!cache?.lines || model.getAlternativeVersionId() !== cache.versionId) {
      this.clear();
      return;
    }
    const line = position.lineNumber;
    if (line > model.getLineCount()) {
      this.clear();
      return;
    }
    const blame = cache.lines.length === 0
      ? { committed: false } as GitBlameInfo
      : cache.lines[line - 1];
    if (!blame) {
      this.clear();
      return;
    }
    this.paint(line, blame);
  }

  private paint(line: number, blame: GitBlameInfo) {
    if (!blame.committed) {
      this.decorate(line, BLAME_UNCOMMITTED, "这一行还没有提交");
      this.publish(BLAME_UNCOMMITTED, "这一行还没有提交");
      return;
    }
    const now = Date.now();
    const hover = blameHoverText(blame, now);
    this.decorate(line, blameEditorText(blame, now), hover);
    this.publish(blameStatusText(blame, now), hover);
  }

  private decorate(line: number, content: string, hover: string) {
    if (this.shown?.line === line && this.shown.content === content && this.shown.hover === hover) return;
    this.shown = { line, content, hover };
    // 用 Monaco 的 injected text，不再为每一行改写全局 <style>。
    const decorations = [{
      range: new monaco.Range(line, Number.MAX_SAFE_INTEGER, line, Number.MAX_SAFE_INTEGER),
      options: {
        after: {
          content,
          inlineClassName: "git-blame-inline",
          cursorStops: monaco.editor.InjectedTextCursorStops.None,
        },
        hoverMessage: { value: hover },
      },
    }];
    if (!this.collection) this.collection = this.editor.createDecorationsCollection(decorations);
    else this.collection.set(decorations);
  }

  private clear() {
    this.shown = null;
    this.collection?.clear();
    this.publish("", "");
  }
}
