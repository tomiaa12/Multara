import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import { GitPeek } from "./git-peek";
import { gitGutterClass, gitGutterHover, gitGutterModel, type GitChange } from "./git-gutter-marks";

const OVERVIEW: Record<GitChange["kind"], string> = {
  added: "rgba(72, 126, 2, 0.6)",
  modified: "rgba(27, 129, 168, 0.6)",
  deleted: "rgba(241, 76, 76, 0.6)",
};

export class GitGutter {
  private collection: Monaco.editor.IEditorDecorationsCollection | null = null;
  private baselineKey = "";
  private baseline: string | null = null;
  private changes: GitChange[] = [];
  private timer = 0;
  private generation = 0;
  private readonly peek: GitPeek;
  private readonly listener: { dispose: () => void };

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly loadBaseline: () => Promise<string | null>,
    private readonly baselineKeyOf: () => string,
    locate: () => { workspaceId: string; filePath: string },
  ) {
    this.peek = new GitPeek(editor, locate);
    this.listener = editor.onMouseUp((event) => this.onMouseUp(event));
  }

  schedule(resetBaseline = false) {
    if (resetBaseline) {
      this.generation += 1;
      this.baselineKey = "";
      this.baseline = null;
      this.peek.close();
    }
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => {
      void this.refresh();
    }, 120);
  }

  dispose() {
    window.clearTimeout(this.timer);
    this.generation += 1;
    this.listener.dispose();
    this.peek.dispose();
    this.collection?.clear();
    this.collection = null;
  }

  private onMouseUp(event: Monaco.editor.IEditorMouseEvent) {
    if (event.target.type !== monaco.editor.MouseTargetType.GUTTER_LINE_DECORATIONS) return;
    if (!hasDirtyGlyph(event.target.element)) return;
    const line = event.target.position?.lineNumber ?? event.target.range?.startLineNumber;
    if (!line) return;
    this.peek.toggle(line);
  }

  private async refresh() {
    const generation = ++this.generation;
    const key = this.baselineKeyOf();
    if (this.baselineKey !== key) {
      let text: string | null = null;
      try {
        text = await this.loadBaseline();
      } catch {
        text = null;
      }
      if (generation !== this.generation) return;
      this.baselineKey = key;
      this.baseline = text;
    }
    if (generation !== this.generation) return;
    this.paint();
  }

  private paint() {
    const model = this.editor.getModel();
    if (this.baseline == null || !model) {
      this.changes = [];
      this.collection?.clear();
      this.peek.sync("", "", []);
      return;
    }
    const current = this.editor.getValue();
    const { marks, changes } = gitGutterModel(this.baseline, current, model.getLineCount());
    this.changes = changes;
    this.peek.sync(this.baseline, current, changes);
    const decorations = marks.map((mark) => ({
      range: new monaco.Range(mark.line, 1, mark.line, 1),
      options: {
        isWholeLine: mark.kind !== "deleted",
        linesDecorationsClassName: gitGutterClass(mark),
        hoverMessage: { value: gitGutterHover(mark.kind) },
        overviewRuler: {
          color: OVERVIEW[mark.kind],
          position: monaco.editor.OverviewRulerLane.Left,
        },
      },
    }));
    if (!this.collection) this.collection = this.editor.createDecorationsCollection(decorations);
    else this.collection.set(decorations);
  }
}

function hasDirtyGlyph(node: HTMLElement | null): boolean {
  let current: HTMLElement | null = node;
  for (let depth = 0; depth < 4 && current; depth += 1) {
    if (current.classList.contains("dirty-diff-glyph")) return true;
    current = current.parentElement;
  }
  return false;
}
