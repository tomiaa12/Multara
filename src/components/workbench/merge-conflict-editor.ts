import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import {
  findMergeConflicts,
  resolveMergeConflict,
  type MergeConflict,
  type MergeConflictChoice,
} from "./merge-conflict";

const ACTIONS: Array<{ choice: MergeConflictChoice; label: string }> = [
  { choice: "current", label: "接受当前更改" },
  { choice: "incoming", label: "接受传入的更改" },
  { choice: "both", label: "接受双方更改" },
];
const CONFLICT_HEADER_SEARCH = "^<<<<<<<";

/** 冲突操作交给 Monaco 的 content widget 定位，不监听布局，避免滚动和粘性滚动互相触发。 */
export class MergeConflictEditor {
  private decorationIds: string[] = [];
  private widgets: Monaco.editor.IContentWidget[] = [];
  private refreshing = false;
  private queued = 0;
  private readonly listeners: Array<{ dispose: () => void }>;

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly disabled: () => boolean,
  ) {
    this.listeners = [
      editor.onDidChangeModelContent(() => this.queue()),
      editor.onDidChangeModel(() => this.refresh()),
    ];
    this.queue();
  }

  refresh() {
    if (this.refreshing) return;
    this.refreshing = true;
    try {
      this.paint();
    } finally {
      this.refreshing = false;
    }
  }

  dispose() {
    if (this.queued) cancelAnimationFrame(this.queued);
    this.queued = 0;
    this.listeners.forEach((item) => item.dispose());
    this.clearWidgets();
    this.decorationIds = this.editor.deltaDecorations(this.decorationIds, []);
  }

  private queue() {
    if (this.queued || this.refreshing) return;
    this.queued = requestAnimationFrame(() => {
      this.queued = 0;
      this.refresh();
    });
  }

  private paint() {
    const model = this.editor.getModel();
    this.clearWidgets();
    // 每次编辑都会走到这里，没有冲突头时不取全文。
    if (!model?.findNextMatch(CONFLICT_HEADER_SEARCH, { lineNumber: 1, column: 1 }, true, true, null, false)) {
      this.decorationIds = this.editor.deltaDecorations(this.decorationIds, []);
      return;
    }
    const conflicts = findMergeConflicts(model.getValue());
    this.decorationIds = this.editor.deltaDecorations(
      this.decorationIds,
      conflicts.flatMap((conflict) => this.decorations(model, conflict)),
    );
    for (const conflict of conflicts) {
      const dom = this.lens(conflict);
      const widget: Monaco.editor.IContentWidget = {
        getId: () => `merge-conflict-${conflict.headerLine}`,
        getDomNode: () => dom,
        getPosition: () => ({
          position: { lineNumber: conflict.headerLine, column: 1 },
          preference: [monaco.editor.ContentWidgetPositionPreference.ABOVE],
        }),
      };
      this.editor.addContentWidget(widget);
      this.widgets.push(widget);
    }
  }

  private decorations(model: Monaco.editor.ITextModel, conflict: MergeConflict): Monaco.editor.IModelDeltaDecoration[] {
    const ancestor = this.ancestorLine(model, conflict);
    const items = [
      this.lineDecoration(model, conflict.headerLine, conflict.headerLine, "merge-conflict-current-header"),
      this.lineDecoration(model, conflict.splitterLine, conflict.splitterLine, "merge-conflict-splitter"),
      this.lineDecoration(model, conflict.footerLine, conflict.footerLine, "merge-conflict-incoming-header"),
    ];
    if (conflict.headerLine + 1 <= ancestor - 1) {
      items.push(this.lineDecoration(model, conflict.headerLine + 1, ancestor - 1, "merge-conflict-current"));
    }
    if (ancestor < conflict.splitterLine) {
      items.push(this.lineDecoration(model, ancestor, conflict.splitterLine - 1, "merge-conflict-ancestor"));
    }
    if (conflict.splitterLine + 1 <= conflict.footerLine - 1) {
      items.push(this.lineDecoration(model, conflict.splitterLine + 1, conflict.footerLine - 1, "merge-conflict-incoming"));
    }
    return items;
  }

  private lineDecoration(
    model: Monaco.editor.ITextModel,
    start: number,
    end: number,
    className: string,
  ): Monaco.editor.IModelDeltaDecoration {
    return {
      range: new monaco.Range(start, 1, end, model.getLineMaxColumn(end)),
      options: { isWholeLine: true, className },
    };
  }

  private ancestorLine(model: Monaco.editor.ITextModel, conflict: MergeConflict) {
    for (let line = conflict.headerLine + 1; line < conflict.splitterLine; line += 1) {
      if (model.getLineContent(line).startsWith("|||||||")) return line;
    }
    return conflict.splitterLine;
  }

  private lens(conflict: MergeConflict) {
    const bar = document.createElement("div");
    bar.className = "merge-conflict-lens";
    for (const action of ACTIONS) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "merge-conflict-lens-action";
      button.textContent = action.label;
      button.disabled = this.disabled();
      button.addEventListener("click", (event) => {
        event.preventDefault();
        event.stopPropagation();
        this.accept(conflict, action.choice);
      });
      bar.append(button);
    }
    return bar;
  }

  private accept(conflict: MergeConflict, choice: MergeConflictChoice) {
    if (this.disabled() || this.refreshing) return;
    const model = this.editor.getModel();
    if (!model) return;
    const next = resolveMergeConflict(model.getValue(), conflict, choice);
    if (next === model.getValue()) return;
    this.editor.executeEdits("merge-conflict", [{
      range: model.getFullModelRange(),
      text: next,
    }]);
  }

  private clearWidgets() {
    for (const widget of this.widgets) this.editor.removeContentWidget(widget);
    this.widgets = [];
  }
}
