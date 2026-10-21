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

let blameStyleSerial = 0;

function blameAfterRule(className: string, content: string): string {
  const text = content
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/[\r\n]/g, " ")
    .replace(/</g, "\\3c ");
  return `.monaco-editor .${className}::after{content:"${text}"}`;
}

export class GitBlame {
  private collection: Monaco.editor.IEditorDecorationsCollection | null = null;
  private timer = 0;
  private generation = 0;
  private shown: { line: number; content: string; hover: string } | null = null;
  private readonly styleClass = `git-blame-text-${++blameStyleSerial}`;
  private readonly style = document.createElement("style");
  private readonly listeners: Array<{ dispose: () => void }> = [];

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly locate: () => { workspaceId: string; filePath: string },
    private readonly publish: (label: string, title: string) => void,
  ) {
    this.style.dataset.gitBlame = this.styleClass;
    document.head.appendChild(this.style);
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
    this.style.remove();
    this.publish("", "");
  }

  private schedule() {
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => {
      void this.refresh();
    }, 180);
  }

  private async refresh() {
    const generation = ++this.generation;
    const position = this.editor.getPosition();
    const model = this.editor.getModel();
    if (!position || !model) {
      this.clear();
      return;
    }
    const { workspaceId, filePath } = this.locate();
    let blame: GitBlameInfo | null = null;
    try {
      blame = await backend.gitBlameLine(workspaceId, filePath, position.lineNumber);
    } catch {
      blame = null;
    }
    if (generation !== this.generation) return;
    if (!blame || position.lineNumber > model.getLineCount()) {
      this.clear();
      return;
    }
    this.paint(position.lineNumber, blame);
  }

  private paint(line: number, blame: GitBlameInfo) {
    const model = this.editor.getModel();
    if (!model) return;
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
    this.style.textContent = blameAfterRule(this.styleClass, content);
    const decorations = [{
      range: new monaco.Range(line, Number.MAX_SAFE_INTEGER, line, Number.MAX_SAFE_INTEGER),
      options: {
        afterContentClassName: `git-blame-after ${this.styleClass}`,
        hoverMessage: { value: hover },
      },
    }];
    if (!this.collection) this.collection = this.editor.createDecorationsCollection(decorations);
    else this.collection.set(decorations);
  }

  private clear() {
    this.shown = null;
    this.style.textContent = "";
    this.collection?.clear();
    this.publish("", "");
  }
}
