import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import "../../assets/codicons/codicon.css";
import { applyLineChanges, revertLineChange } from "./git-line-changes";
import { gitChangeAtLine, gitPeekBodyLines, type GitChange } from "./git-gutter-marks";

const PEEK_HEADER = 26;
const PEEK_OVERLAY_ID = "multara.git-peek";

const FRAME: Record<GitChange["kind"], string> = {
  added: "#487E02",
  modified: "#1B81A8",
  deleted: "#F14C4C",
};

export class GitPeek {
  private zoneId: string | null = null;
  private overlay: Monaco.editor.IOverlayWidget | null = null;
  private root: HTMLElement | null = null;
  private index = -1;
  private changes: GitChange[] = [];
  private bodyEl: HTMLElement | null = null;
  private bodyHeight = 0;
  private diff: Monaco.editor.IStandaloneDiffEditor | null = null;
  private originalModel: Monaco.editor.ITextModel | null = null;
  private baselineText = "";
  private layoutDisposable: { dispose: () => void } | null = null;
  private busy = false;

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly locate: () => { workspaceId: string; filePath: string },
    private readonly onMutate: () => void,
  ) {}

  toggle(line: number) {
    const next = gitChangeAtLine(this.changes, line);
    if (next < 0) return;
    if (next === this.index) this.close();
    else this.show(next);
  }

  sync(baseline: string, _current: string, changes: GitChange[]) {
    this.changes = changes;
    this.baselineText = baseline;
    if (this.index < 0) return;
    const change = changes[this.index];
    if (!change) {
      this.close();
      return;
    }
    this.renderTitle();
    this.fillBody(change);
  }

  close() {
    this.index = -1;
    this.bodyEl = null;
    this.bodyHeight = 0;
    this.diff?.dispose();
    this.diff = null;
    this.originalModel?.dispose();
    this.originalModel = null;
    this.layoutDisposable?.dispose();
    this.layoutDisposable = null;
    if (this.overlay) {
      this.editor.removeOverlayWidget(this.overlay);
      this.overlay = null;
    }
    this.root = null;
    if (this.zoneId) {
      const id = this.zoneId;
      this.zoneId = null;
      this.editor.changeViewZones((accessor) => accessor.removeZone(id));
    }
  }

  dispose() {
    this.close();
  }

  private show(index: number) {
    const change = this.changes[index];
    if (!change) return;
    this.close();
    this.index = index;
    const lineHeight = this.editor.getOption(monaco.editor.EditorOption.lineHeight);
    // 同 VSCode：改动行数 + 6 行上下文，最多占编辑器高度的 1/3。
    const editorLines = Math.floor(this.editor.getLayoutInfo().height / lineHeight);
    const bodyLines = Math.max(3, Math.min(gitPeekBodyLines(change) + 6, Math.floor(editorLines / 3)));
    this.bodyHeight = bodyLines * lineHeight;
    const totalHeight = PEEK_HEADER + this.bodyHeight;

    const root = this.build(change);
    root.style.setProperty("--peek-frame", FRAME[change.kind]);
    root.style.top = "-1000px";
    root.style.height = `${totalHeight}px`;
    root.style.background = this.editorBackground();
    if (this.bodyEl) this.bodyEl.style.height = `${this.bodyHeight}px`;
    this.root = root;
    this.layoutOverlay();

    // View zone 只占高度；真正 UI 挂在 overlay，才能通栏盖住行号区（同 ZoneWidget）。
    const spacer = document.createElement("div");
    spacer.style.overflow = "hidden";
    let zoneId = "";
    this.editor.changeViewZones((accessor) => {
      zoneId = accessor.addZone({
        afterLineNumber: change.endLine,
        heightInPx: totalHeight,
        domNode: spacer,
        onDomNodeTop: (top) => {
          if (this.root) this.root.style.top = `${top}px`;
        },
        onComputedHeight: (height) => {
          if (!this.root) return;
          this.root.style.height = `${height}px`;
          this.layoutDiff();
        },
      });
    });
    this.zoneId = zoneId;

    const overlay: Monaco.editor.IOverlayWidget = {
      getId: () => PEEK_OVERLAY_ID,
      getDomNode: () => root,
      getPosition: () => null,
    };
    this.overlay = overlay;
    this.editor.addOverlayWidget(overlay);
    this.layoutDisposable = this.editor.onDidLayoutChange(() => {
      this.layoutOverlay();
      this.layoutDiff();
    });

    this.fillBody(change);
    this.editor.revealLineInCenterIfOutsideViewport(change.endLine);
  }

  private layoutOverlay() {
    const root = this.root;
    if (!root) return;
    const info = this.editor.getLayoutInfo();
    const left = info.minimap.minimapWidth > 0 && info.minimap.minimapLeft === 0 ? info.minimap.minimapWidth : 0;
    const width = info.width - info.minimap.minimapWidth - info.verticalScrollbarWidth;
    root.style.left = `${left}px`;
    root.style.width = `${Math.max(0, width)}px`;
    root.style.position = "absolute";
  }

  private layoutDiff() {
    if (!this.diff || !this.bodyEl) return;
    const width = this.bodyEl.clientWidth;
    const height = this.bodyHeight || this.bodyEl.clientHeight;
    if (width <= 0 || height <= 0) return;
    this.diff.layout({ width, height });
  }

  private editorBackground(): string {
    const node = this.editor.getDomNode()?.querySelector<HTMLElement>(".monaco-editor-background");
    const color = node ? getComputedStyle(node).backgroundColor : "";
    return color && color !== "rgba(0, 0, 0, 0)" && color !== "transparent" ? color : "var(--color-surface)";
  }

  private build(change: GitChange): HTMLElement {
    const root = document.createElement("div");
    // 滚轮留给弹窗内部：不让事件冒泡到外层编辑器，否则外层会抢走滚动。
    root.addEventListener("wheel", (event) => event.stopPropagation(), { passive: true });
    root.className = `dirty-diff-peek dirty-diff-peek-${change.kind}`;
    const title = document.createElement("div");
    title.className = "dirty-diff-peek-title";
    const label = document.createElement("span");
    label.className = "dirty-diff-peek-label";
    const detail = document.createElement("span");
    detail.className = "dirty-diff-peek-detail";
    const actions = document.createElement("span");
    actions.className = "dirty-diff-peek-actions";
    // 同 VS Code：add / discard / arrowDown / arrowUp / close。
    actions.append(
      this.action(t("scm.peek.stageChange"), "add", () => void this.stageChange()),
      this.action(t("scm.peek.revertChange"), "discard", () => this.revertChange()),
      this.action(t("scm.peek.nextChange"), "arrow-down", () => this.step(1)),
      this.action(t("scm.peek.prevChange"), "arrow-up", () => this.step(-1)),
      this.action(t("common.close"), "close", () => this.close()),
    );
    title.append(label, detail, actions);
    const body = document.createElement("div");
    body.className = "dirty-diff-peek-body";
    this.bodyEl = body;
    root.append(title, body);
    this.renderTitle(label, detail);
    return root;
  }

  /** 同 VSCode 的 EmbeddedDiffEditorWidget：非并排的内嵌 diff，自带滚动和语法高亮。 */
  private fillBody(change: GitChange) {
    const body = this.bodyEl;
    const model = this.editor.getModel();
    if (!body || !model) return;
    if (!this.diff) {
      this.diff = monaco.editor.createDiffEditor(body, {
        renderSideBySide: false,
        readOnly: true,
        originalEditable: false,
        automaticLayout: false,
        renderIndicators: false,
        renderOverviewRuler: false,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        ignoreTrimWhitespace: false,
        hideUnchangedRegions: { enabled: false },
        fontSize: this.editor.getOption(monaco.editor.EditorOption.fontSize),
        fontFamily: this.editor.getOption(monaco.editor.EditorOption.fontFamily),
        lineHeight: this.editor.getOption(monaco.editor.EditorOption.lineHeight),
        scrollbar: {
          verticalScrollbarSize: 14,
          useShadows: true,
          horizontal: "auto",
          alwaysConsumeMouseWheel: true,
        },
        stickyScroll: { enabled: false },
      });
      this.originalModel = monaco.editor.createModel(this.baselineText, model.getLanguageId());
      this.diff.setModel({ original: this.originalModel, modified: model });
      this.layoutDiff();
      const reveal = this.diff.onDidUpdateDiff(() => {
        reveal.dispose();
        window.setTimeout(() => {
          this.layoutDiff();
          this.revealChange(change);
        }, 0);
      });
    } else if (this.originalModel && this.originalModel.getValue() !== this.baselineText) {
      this.originalModel.setValue(this.baselineText);
      this.layoutDiff();
    }
  }

  private revealChange(change: GitChange) {
    const modified = this.diff?.getModifiedEditor();
    if (!modified) return;
    let start = change.startLine;
    let end = change.endLine;
    if (change.kind === "modified") {
      start -= 1;
      end += 1;
    } else if (change.kind === "deleted") {
      end = start + 1;
    }
    modified.revealLinesInCenter(Math.max(1, start), Math.max(1, end), monaco.editor.ScrollType.Immediate);
  }

  private renderTitle(label?: HTMLElement, detail?: HTMLElement) {
    const root = this.root;
    const labelEl = label ?? root?.querySelector(".dirty-diff-peek-label");
    const detailEl = detail ?? root?.querySelector(".dirty-diff-peek-detail");
    const { filePath } = this.locate();
    const name = filePath.split(/[/\\]/).pop() || filePath;
    if (labelEl) labelEl.textContent = name;
    if (detailEl && this.index >= 0) {
      detailEl.textContent = t("scm.peek.workspaceChanges", {
        current: this.index + 1,
        total: this.changes.length,
      });
    }
  }

  private action(title: string, icon: string, onClick: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "dirty-diff-peek-action";
    button.title = title;
    const glyph = document.createElement("span");
    glyph.className = `codicon codicon-${icon}`;
    glyph.setAttribute("aria-hidden", "true");
    button.append(glyph);
    button.addEventListener("mousedown", (event) => event.preventDefault());
    button.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      onClick();
    });
    return button;
  }

  private step(delta: number) {
    if (this.changes.length === 0 || this.index < 0) return;
    const next = (this.index + delta + this.changes.length) % this.changes.length;
    this.show(next);
  }

  private revertChange() {
    const change = this.changes[this.index];
    const model = this.editor.getModel();
    if (!change || !model || this.busy) return;
    const next = revertLineChange(model.getValue(), change);
    const full = model.getFullModelRange();
    this.editor.pushUndoStop();
    this.editor.executeEdits("git-peek-revert", [{ range: full, text: next }]);
    this.editor.pushUndoStop();
    this.close();
  }

  private async stageChange() {
    const change = this.changes[this.index];
    if (!change || this.busy) return;
    const { workspaceId, filePath } = this.locate();
    const contents = applyLineChanges(this.baselineText, [change]);
    this.busy = true;
    try {
      await backend.gitStageContents(workspaceId, filePath, contents);
      this.close();
      this.onMutate();
    } catch (error) {
      console.error(error);
    } finally {
      this.busy = false;
    }
  }
}
