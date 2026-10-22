import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import { blameAgo, type GitBlameInfo } from "./git-blame";
import { gitChangeAtLine, gitPeekBodyLines, type GitChange } from "./git-gutter-marks";

const PEEK_HEADER = 26;

const FRAME: Record<GitChange["kind"], string> = {
  added: "#487E02",
  modified: "#1B81A8",
  deleted: "#F14C4C",
};

export class GitPeek {
  private zoneId: string | null = null;
  private index = -1;
  private token = 0;
  private changes: GitChange[] = [];
  private recordEl: HTMLElement | null = null;
  private bodyEl: HTMLElement | null = null;

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly locate: () => { workspaceId: string; filePath: string },
  ) {}

  toggle(line: number) {
    const next = gitChangeAtLine(this.changes, line);
    if (next < 0) return;
    if (next === this.index) this.close();
    else this.show(next);
  }

  sync(_baseline: string, _current: string, changes: GitChange[]) {
    this.changes = changes;
    if (this.index < 0) return;
    const change = changes[this.index];
    if (!change) {
      this.close();
      return;
    }
    this.fillBody(change);
  }

  close() {
    this.token += 1;
    this.index = -1;
    this.recordEl = null;
    this.bodyEl = null;
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
    const bodyHeight = gitPeekBodyLines(change) * lineHeight;
    const root = this.build(change, lineHeight);
    root.style.setProperty("--peek-frame", FRAME[change.kind]);
    root.style.height = `${PEEK_HEADER + bodyHeight}px`;
    if (this.bodyEl) this.bodyEl.style.height = `${bodyHeight}px`;
    let zoneId = "";
    this.editor.changeViewZones((accessor) => {
      zoneId = accessor.addZone({
        afterLineNumber: change.endLine,
        heightInPx: PEEK_HEADER + bodyHeight,
        domNode: root,
        suppressMouseDown: true,
      });
    });
    this.zoneId = zoneId;
    this.editor.revealLineInCenterIfOutsideViewport(change.endLine);
    void this.loadRecord(change, this.token);
  }

  private build(change: GitChange, lineHeight: number): HTMLElement {
    const root = document.createElement("div");
    root.className = `dirty-diff-peek dirty-diff-peek-${change.kind}`;
    const title = document.createElement("div");
    title.className = "dirty-diff-peek-title";
    const label = document.createElement("span");
    label.className = "dirty-diff-peek-label";
    const detail = document.createElement("span");
    detail.className = "dirty-diff-peek-detail";
    const record = document.createElement("span");
    record.className = "dirty-diff-peek-record";
    this.recordEl = record;
    const actions = document.createElement("span");
    actions.className = "dirty-diff-peek-actions";
    actions.append(
      this.action(t("scm.peek.prevChange"), "↑", () => this.step(-1)),
      this.action(t("scm.peek.nextChange"), "↓", () => this.step(1)),
      this.action(t("common.close"), "×", () => this.close()),
    );
    title.append(label, detail, record, actions);
    const body = document.createElement("div");
    body.className = "dirty-diff-peek-body vs-scroll";
    const fontSize = this.editor.getOption(monaco.editor.EditorOption.fontSize);
    const fontFamily = this.editor.getOption(monaco.editor.EditorOption.fontFamily);
    body.style.fontSize = `${fontSize}px`;
    body.style.fontFamily = fontFamily;
    this.bodyEl = body;
    root.append(title, body);
    this.renderTitle(label, detail);
    this.fillBody(change, lineHeight);
    return root;
  }

  private fillBody(change: GitChange, lineHeight = this.editor.getOption(monaco.editor.EditorOption.lineHeight)) {
    const body = this.bodyEl;
    if (!body) return;
    body.replaceChildren();
    for (const line of change.oldLines) body.append(this.lineRow("deleted", line, lineHeight));
    for (const line of change.newLines) body.append(this.lineRow("added", line, lineHeight));
  }

  private lineRow(kind: "deleted" | "added", text: string, lineHeight: number): HTMLElement {
    const row = document.createElement("div");
    row.className = `dirty-diff-peek-line ${kind}`;
    row.style.height = `${lineHeight}px`;
    row.style.lineHeight = `${lineHeight}px`;
    row.textContent = text.length > 0 ? text : " ";
    return row;
  }

  private renderTitle(label?: HTMLElement, detail?: HTMLElement) {
    const root = this.recordEl?.parentElement?.parentElement;
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

  private action(title: string, text: string, onClick: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "dirty-diff-peek-action";
    button.title = title;
    button.textContent = text;
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

  private async loadRecord(change: GitChange, token: number) {
    if (!this.recordEl || change.oldLines.length === 0 || change.oldStart < 1) return;
    const { workspaceId, filePath } = this.locate();
    let blame: GitBlameInfo | null = null;
    try {
      blame = await backend.gitBlameLine(workspaceId, filePath, change.oldStart, "HEAD");
    } catch {
      blame = null;
    }
    if (token !== this.token || !this.recordEl || !blame?.committed) return;
    this.recordEl.textContent = `${blame.subject}，${blame.authorName}（${blameAgo(blame.authorTime, Date.now())}）`;
    this.recordEl.title = blame.hash;
  }
}
