import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import * as backend from "../../services/backend";
import * as explorerBackend from "../../services/explorer-backend";
import {
  blameEditorText,
  blameExplainPrompt,
  blameLineDiffSnippet,
  blameStatusText,
  blameStatusTitle,
  remoteCommitWebUrl,
  type BlameDiffLine,
  type GitBlameInfo,
} from "./git-blame";
import {
  GitBlameCard,
  type BlameCardAction,
  type BlameCardAnchor,
  type BlameCardModel,
} from "./git-blame-card";
import { mapCurrentLineToBlameLine } from "./git-blame-map";

export type BlameWorkbenchAction =
  | { type: "open-changes"; blame: GitBlameInfo }
  | { type: "blame-prior"; blame: GitBlameInfo }
  | { type: "reveal"; workspaceId: string; filePath: string }
  | { type: "remote"; url: string }
  | { type: "graph"; workspaceId: string; hash: string }
  | { type: "explain"; text: string };

/** Monaco 行尾装饰列上限（同 GitLens / VS Code maxSmallIntegerV8）。 */
const LINE_END_COLUMN = 2 ** 30 - 1;
const REFRESH_MS = 50;

export class GitBlame {
  private current: GitBlameInfo | null = null;
  private currentLine = 0;
  private blameLine = 0;
  private currentDiff: BlameDiffLine[] = [];
  private remoteUrl: string | null = null;
  private detailsToken = 0;
  private detailsLoadedFor = "";

  private collection: Monaco.editor.IEditorDecorationsCollection | null = null;
  private styleEl: HTMLStyleElement | null = null;
  private timer = 0;
  private generation = 0;
  private cacheToken = 0;
  private shown: { line: number; content: string } | null = null;
  /** 整文件 blame 缓存：移动光标只查表。 */
  private cache: {
    key: string;
    baseText: string;
    lines: GitBlameInfo[] | null;
  } | null = null;
  private loading: Promise<void> | null = null;
  private remoteCache: { workspaceId: string; urlBase: string | null } | null = null;
  private readonly listeners: Array<{ dispose: () => void }> = [];
  private readonly card: GitBlameCard;

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly locate: () => { workspaceId: string; filePath: string },
    private readonly publish: (label: string, title: string) => void,
    private readonly cacheKey: () => string = () => "",
    private readonly onAction: (action: BlameWorkbenchAction) => void = () => {},
  ) {
    this.card = new GitBlameCard(editor, (action, model) => this.handleCardAction(action, model));
    this.listeners.push(
      editor.onDidChangeCursorPosition((event) => {
        const lineChanged = event.position.lineNumber !== this.currentLine;
        if (!lineChanged) return;
        // 同 VS Code ContentHover：选区换行立刻收起 hover，避免卡在旧位置。
        this.card.hide();
        // 同 GitLens pending：先清旧行尾，blame 就绪后再画。
        this.clearAnnotation();
        this.current = null;
        this.currentLine = 0;
        this.schedule();
      }),
      editor.onDidChangeModelContent(() => {
        this.card.hide();
        this.schedule();
      }),
      editor.onDidChangeModel(() => {
        this.invalidateCache();
        this.card.hide();
        this.clearAnnotation();
        this.current = null;
        this.currentLine = 0;
        this.blameLine = 0;
        this.currentDiff = [];
        this.remoteUrl = null;
        this.detailsLoadedFor = "";
        this.publish("", "");
        this.schedule();
      }),
      editor.onMouseDown((event) => {
        // 同 VS Code：mousedown 收起 content hover；点在卡片上保留。
        if (event.target.element?.closest?.(".git-blame-card")) return;
        this.card.hide();
      }),
      editor.onMouseMove((event) => this.onMouseMove(event)),
      editor.onMouseLeave(() => this.card.scheduleHide()),
    );
    this.schedule();
  }

  dispose() {
    window.clearTimeout(this.timer);
    this.generation += 1;
    this.cacheToken += 1;
    this.detailsToken += 1;
    this.listeners.splice(0).forEach((item) => item.dispose());
    this.card.dispose();
    this.clearAnnotation();
    this.collection = null;
    this.styleEl?.remove();
    this.styleEl = null;
    this.cache = null;
    this.publish("", "");
  }

  refreshSoon() {
    this.invalidateCache();
    this.remoteCache = null;
    this.schedule();
  }

  private invalidateCache() {
    this.cacheToken += 1;
    this.cache = null;
    this.detailsLoadedFor = "";
    this.loading = null;
  }

  private schedule() {
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => {
      void this.refresh();
    }, REFRESH_MS);
  }

  private async ensureCache(model: Monaco.editor.ITextModel) {
    const { workspaceId, filePath } = this.locate();
    const key = `${workspaceId}\0${filePath}\0${this.cacheKey()}`;
    if (this.cache?.key === key) return;

    const token = ++this.cacheToken;
    const loading = (async () => {
      let lines: GitBlameInfo[] | null = null;
      let baseText = model.getValue();
      try {
        const [blameLines, disk] = await Promise.all([
          backend.gitBlameFile(workspaceId, filePath),
          backend.readWorkspaceFile(workspaceId, filePath).catch(() => null),
        ]);
        lines = blameLines;
        if (disk?.content != null) baseText = disk.content;
      } catch {
        lines = null;
      }
      // 过期请求不得写回，避免切文件后被旧 blame 卡住。
      if (token !== this.cacheToken) return;
      this.cache = { key, baseText, lines };
    })();
    this.loading = loading;
    await loading;
    if (this.loading === loading) this.loading = null;
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
    const { workspaceId, filePath } = this.locate();
    const key = `${workspaceId}\0${filePath}\0${this.cacheKey()}`;
    if (!cache || cache.key !== key) {
      this.clear();
      return;
    }
    if (!cache.lines) {
      this.clear();
      return;
    }
    const currentLine = position.lineNumber;
    if (currentLine > model.getLineCount()) {
      this.clear();
      return;
    }
    const blameLine = mapCurrentLineToBlameLine(cache.baseText, model.getValue(), currentLine);
    if (blameLine == null || blameLine < 1) {
      this.clear();
      return;
    }
    const blame = cache.lines.length === 0
      ? emptyUncommitted()
      : cache.lines[blameLine - 1];
    if (!blame) {
      this.clear();
      return;
    }
    this.paint(currentLine, blameLine, blame);
  }

  private paint(viewLine: number, blameLine: number, blame: GitBlameInfo) {
    this.current = blame;
    this.currentLine = viewLine;
    this.blameLine = blameLine;
    const key = `${blame.hash}\0${blameLine}\0${viewLine}`;
    if (this.detailsLoadedFor !== key) {
      this.currentDiff = [];
      this.remoteUrl = null;
      this.detailsLoadedFor = "";
    }
    const now = Date.now();
    this.decorate(viewLine, blameEditorText(blame, now));
    this.publish(blameStatusText(blame, now), blameStatusTitle(blame, now));
  }

  /**
   * 同 GitLens / VS Code：afterContentClassName + CSS ::after contentText。
   * 文案不进 TextModel / InjectedText，不影响编辑与光标。
   * 悬停不靠命中 ::after DOM，而靠 MouseTarget 锚到行尾 EOL（同 HoverProvider）。
   */
  private decorate(line: number, content: string) {
    if (this.shown?.line === line && this.shown.content === content) return;
    this.shown = { line, content };
    this.writeAfterCss(content);
    const decorations = [{
      range: new monaco.Range(line, LINE_END_COLUMN, line, LINE_END_COLUMN),
      options: {
        description: "git-blame-inline",
        afterContentClassName: "git-blame-after",
      },
    }];
    if (!this.collection) this.collection = this.editor.createDecorationsCollection(decorations);
    else this.collection.set(decorations);
  }

  private writeAfterCss(content: string) {
    if (!this.styleEl) {
      this.styleEl = document.createElement("style");
      this.styleEl.dataset.multaraGitBlame = "1";
      document.head.append(this.styleEl);
    }
    if (!content) {
      this.styleEl.textContent = "";
      return;
    }
    // 同 VS Code DecorationOverviewRuler / after contentText：::after 撑开 lineWidth，
    // 鼠标落在文案上时 MouseTarget 仍解析为 EOL（column === lineMaxColumn）。
    this.styleEl.textContent =
      `.monaco-editor .view-lines .git-blame-after::after{content:"${escapeCssContent(content)}"}`;
  }

  private clearAnnotation() {
    this.shown = null;
    this.collection?.clear();
    this.writeAfterCss("");
  }

  private onMouseMove(event: Monaco.editor.IEditorMouseEvent) {
    if (event.target.element?.closest?.(".git-blame-card")) {
      this.card.cancelHide();
      return;
    }
    if (!this.current || !this.currentLine) {
      this.card.scheduleHide();
      return;
    }
    // 有文字行上悬停 ::after 时，Monaco 有时不给 position（空行反而稳定给 EOL）。
    // 同 VS Code：用坐标落入当前行 + 行尾以右（注解撑开区）判定，不依赖 DOM class。
    const line = event.target.position?.lineNumber
      ?? event.target.range?.startLineNumber
      ?? this.lineAtMouseY(event)
      ?? 0;
    if (!line || line !== this.currentLine) {
      this.card.scheduleHide();
      return;
    }
    if (!this.isAtLineEndAnnotation(event, line)) {
      this.card.scheduleHide();
      return;
    }
    this.card.cancelHide();
    void this.openCard(line);
  }

  private lineAtMouseY(event: Monaco.editor.IEditorMouseEvent): number | null {
    const mouse = this.mouseInEditor(event);
    if (!mouse || !this.currentLine) return null;
    const top = this.editor.getTopForLineNumber(this.currentLine) - this.editor.getScrollTop();
    const lineHeight = this.editor.getOption(monaco.editor.EditorOption.lineHeight);
    if (mouse.y < top - 2 || mouse.y > top + lineHeight + 2) return null;
    return this.currentLine;
  }

  /**
   * 同 GitLens HoverProvider：锚在行尾。
   * 有文字时 ::after 在 EOL 右侧，仅靠 column===eol 会漏（空行 eol=1 反而易命中整行）；
   * 用「当前行 + 鼠标在真实文本右缘以右」覆盖注解区（含 3em margin）。
   */
  private isAtLineEndAnnotation(event: Monaco.editor.IEditorMouseEvent, line: number): boolean {
    const model = this.editor.getModel();
    if (!model) return false;
    const eol = model.getLineMaxColumn(line);
    const column = event.target.position?.column;
    if (column === eol || (column != null && column > eol)) return true;
    if (event.target.mouseColumn > eol) return true;

    const mouse = this.mouseInEditor(event);
    const end = this.editor.getScrolledVisiblePosition({ lineNumber: line, column: eol });
    if (!mouse || !end) return false;
    const lineHeight = this.editor.getOption(monaco.editor.EditorOption.lineHeight);
    if (mouse.y < end.top - 2 || mouse.y > end.top + lineHeight + 2) return false;
    // 文本右缘以右 = after 注解区（CSS margin-left: 3em + content）
    return mouse.x >= end.left;
  }

  private mouseInEditor(event: Monaco.editor.IEditorMouseEvent): BlameCardAnchor | null {
    const dom = this.editor.getDomNode();
    if (!dom) return null;
    const rect = dom.getBoundingClientRect();
    const be = event.event.browserEvent;
    return {
      x: be.clientX - rect.left,
      y: be.clientY - rect.top,
    };
  }

  /** 同 VS Code hover：锚在当前行行尾（注解处），卡片左下角落在该点上方。 */
  private annotationAnchor(line: number): BlameCardAnchor | null {
    const model = this.editor.getModel();
    if (!model) return null;
    const column = model.getLineMaxColumn(line);
    const pos = this.editor.getScrolledVisiblePosition({ lineNumber: line, column });
    if (!pos) return null;
    return { x: pos.left, y: pos.top };
  }

  private async openCard(line: number) {
    if (!this.current) return;
    const anchor = this.annotationAnchor(line);
    // 先立刻弹出，再补 diff，避免 ensureDetails 等待期间悬停丢失。
    this.card.show({
      blame: this.current,
      line,
      diff: this.currentDiff,
      remoteUrl: this.remoteUrl,
    }, anchor);
    await this.ensureDetails(this.current, this.blameLine, line);
    if (!this.current || this.currentLine !== line || !this.card.visible()) return;
    this.card.show({
      blame: this.current,
      line,
      diff: this.currentDiff,
      remoteUrl: this.remoteUrl,
    }, anchor);
  }

  /** 仅在真正打开卡片时拉 diff / remote，避免光标移动就打 git。 */
  private async ensureDetails(blame: GitBlameInfo, blameLine: number, viewLine: number) {
    const key = `${blame.hash}\0${blameLine}\0${viewLine}`;
    if (this.detailsLoadedFor === key) return;
    const token = ++this.detailsToken;
    const { workspaceId, filePath } = this.locate();
    const model = this.editor.getModel();
    const currentText = model?.getValue() ?? "";
    const baseText = this.cache?.baseText ?? currentText;

    let diff: BlameDiffLine[] = [];
    try {
      if (!blame.committed) {
        const head = await backend.gitHeadFile(workspaceId, filePath);
        diff = blameLineDiffSnippet(head.content ?? "", currentText, viewLine);
      } else if (blame.previous) {
        const [oldFile, newFile] = await Promise.all([
          backend.gitRevisionFile(workspaceId, blame.previous, filePath),
          backend.gitRevisionFile(workspaceId, blame.hash, filePath),
        ]);
        diff = blameLineDiffSnippet(oldFile.content ?? "", newFile.content ?? "", blameLine);
      } else {
        const revision = await backend.gitRevisionFile(workspaceId, blame.hash, filePath);
        diff = blameLineDiffSnippet("", revision.content ?? "", blameLine);
      }
    } catch {
      diff = blameLineDiffSnippet(baseText, currentText, viewLine);
    }
    if (token !== this.detailsToken || this.current !== blame) return;
    this.currentDiff = diff;

    let remoteUrl: string | null = null;
    if (blame.committed) {
      try {
        if (this.remoteCache?.workspaceId !== workspaceId) {
          const info = await explorerBackend.gitRemote(workspaceId);
          this.remoteCache = {
            workspaceId,
            urlBase: info?.remote ?? null,
          };
        }
        remoteUrl = this.remoteCache.urlBase
          ? remoteCommitWebUrl(this.remoteCache.urlBase, blame.hash)
          : null;
      } catch {
        remoteUrl = null;
      }
    }
    if (token !== this.detailsToken || this.current !== blame) return;
    this.remoteUrl = remoteUrl;
    this.detailsLoadedFor = key;
  }

  private handleCardAction(action: BlameCardAction, model: BlameCardModel) {
    const { workspaceId, filePath } = this.locate();
    const blame = model.blame;
    this.card.hide();
    if (action === "copy") {
      if (blame.committed) void navigator.clipboard?.writeText(blame.hash);
      return;
    }
    if (action === "changes") {
      this.onAction({ type: "open-changes", blame });
      return;
    }
    if (action === "blame-prior") {
      if (blame.previous) this.onAction({ type: "blame-prior", blame });
      return;
    }
    if (action === "reveal") {
      this.onAction({ type: "reveal", workspaceId, filePath });
      return;
    }
    if (action === "remote") {
      if (model.remoteUrl) this.onAction({ type: "remote", url: model.remoteUrl });
      return;
    }
    if (action === "graph") {
      if (blame.committed) this.onAction({ type: "graph", workspaceId, hash: blame.hash });
      return;
    }
    if (action === "explain") {
      this.onAction({
        type: "explain",
        text: blameExplainPrompt({
          filePath,
          subject: blame.committed ? blame.subject : "",
          hash: blame.committed ? blame.hash : "",
          diff: model.diff,
        }),
      });
    }
  }

  private clear() {
    this.current = null;
    this.currentLine = 0;
    this.blameLine = 0;
    this.currentDiff = [];
    this.remoteUrl = null;
    this.detailsLoadedFor = "";
    this.clearAnnotation();
    this.card.hide();
    this.publish("", "");
  }
}

function emptyUncommitted(): GitBlameInfo {
  return {
    committed: false,
    hash: "",
    authorName: "",
    authorEmail: "",
    authorTime: Math.floor(Date.now() / 1000),
    subject: "",
    previous: "",
  };
}

function escapeCssContent(value: string): string {
  return value
    .replace(/\\/g, "\\\\")
    .replace(/"/g, "\\\"")
    .replace(/\n/g, "\\A ")
    .replace(/\r/g, "")
    .replace(/[\u0000-\u001f]/g, "");
}
