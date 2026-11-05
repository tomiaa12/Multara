import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import "../../assets/codicons/codicon.css";
import { t } from "../../i18n";
import { authorInitial, gravatarUrl } from "../../utils/gravatar";
import {
  blameAbsoluteTime,
  blameAgo,
  blameAuthorLabel,
  blameSubjectLabel,
  type BlameDiffLine,
  type GitBlameInfo,
} from "./git-blame";

const WIDGET_ID = "multara.git-blame-card";

export type BlameCardAction =
  | "copy"
  | "changes"
  | "blame-prior"
  | "reveal"
  | "remote"
  | "graph"
  | "explain";

export interface BlameCardModel {
  blame: GitBlameInfo;
  line: number;
  diff: BlameDiffLine[];
  remoteUrl: string | null;
}

/** 编辑器 DOM 内坐标：弹出层左下角锚在此点。 */
export interface BlameCardAnchor {
  x: number;
  y: number;
}

export class GitBlameCard {
  private overlay: Monaco.editor.IOverlayWidget | null = null;
  private root: HTMLElement | null = null;
  private model: BlameCardModel | null = null;
  private signature = "";
  private anchor: BlameCardAnchor | null = null;
  private hideTimer = 0;
  private readonly onScroll: { dispose: () => void };
  private readonly onLayout: { dispose: () => void };

  constructor(
    private readonly editor: Monaco.editor.IStandaloneCodeEditor,
    private readonly onAction: (action: BlameCardAction, model: BlameCardModel) => void,
  ) {
    this.onScroll = editor.onDidScrollChange(() => this.reposition());
    this.onLayout = editor.onDidLayoutChange(() => this.reposition());
  }

  show(model: BlameCardModel, anchor?: BlameCardAnchor | null) {
    window.clearTimeout(this.hideTimer);
    // 每次展示都更新锚点，避免切行后仍停在旧坐标。
    if (anchor) this.anchor = anchor;
    else if (!this.anchor) this.anchor = fallbackAnchor(this.editor, model.line);
    const next = cardSignature(model);
    const same = this.model && this.signature === next;
    this.model = model;
    this.signature = next;
    if (!this.root) {
      this.root = this.build();
      this.overlay = {
        getId: () => WIDGET_ID,
        getDomNode: () => this.root!,
        getPosition: () => null,
      };
      this.editor.addOverlayWidget(this.overlay);
    }
    if (!same) this.render();
    this.reposition();
    // 首帧测高后再对齐左下角锚点，避免 height=0 时顶到错误位置。
    requestAnimationFrame(() => {
      if (this.root && this.model) this.reposition();
    });
  }

  visible(): boolean {
    return Boolean(this.root && this.model);
  }

  scheduleHide() {
    window.clearTimeout(this.hideTimer);
    this.hideTimer = window.setTimeout(() => this.hide(), 220);
  }

  cancelHide() {
    window.clearTimeout(this.hideTimer);
  }

  hide() {
    window.clearTimeout(this.hideTimer);
    this.model = null;
    this.signature = "";
    this.anchor = null;
    if (this.overlay) {
      this.editor.removeOverlayWidget(this.overlay);
      this.overlay = null;
    }
    this.root = null;
  }

  dispose() {
    this.hide();
    this.onScroll.dispose();
    this.onLayout.dispose();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "git-blame-card";
    root.addEventListener("mouseenter", () => this.cancelHide());
    root.addEventListener("mouseleave", () => this.scheduleHide());
    return root;
  }

  private render() {
    const root = this.root;
    const model = this.model;
    if (!root || !model) return;
    const { blame, diff, remoteUrl } = model;
    const now = Date.now();
    const author = blameAuthorLabel(blame);
    const ago = blameAgo(blame.authorTime, now);
    const when = blameAbsoluteTime(blame.authorTime, now);
    const subject = blameSubjectLabel(blame);
    const short = blame.committed ? blame.hash.slice(0, 7) : "";
    const previous = blame.previous ? blame.previous.slice(0, 7) : "";
    const avatar = gravatarUrl(blame.authorEmail, 32);
    const committed = blame.committed;
    const hasPrevious = Boolean(blame.previous);

    root.replaceChildren();

    const head = document.createElement("div");
    head.className = "git-blame-card-head";

    const avatarEl = document.createElement("div");
    avatarEl.className = "git-blame-card-avatar";
    if (avatar) {
      const img = document.createElement("img");
      img.src = avatar;
      img.alt = "";
      img.referrerPolicy = "no-referrer";
      img.loading = "lazy";
      img.addEventListener("error", () => {
        img.remove();
        avatarEl.textContent = authorInitial(author);
      });
      avatarEl.append(img);
    } else {
      avatarEl.textContent = authorInitial(author);
    }

    const meta = document.createElement("div");
    meta.className = "git-blame-card-meta";
    const who = document.createElement("div");
    who.className = "git-blame-card-who";
    const name = document.createElement("strong");
    name.className = "git-blame-card-author";
    name.textContent = author;
    const time = document.createElement("span");
    time.className = "git-blame-card-time";
    const clock = document.createElement("span");
    clock.className = "codicon codicon-history";
    clock.setAttribute("aria-hidden", "true");
    time.append(clock, document.createTextNode(` ${ago} (${when})`));
    who.append(name, time);
    meta.append(who);
    head.append(avatarEl, meta);

    const message = document.createElement("div");
    message.className = "git-blame-card-message";
    message.textContent = subject;

    const actions = document.createElement("div");
    actions.className = "git-blame-card-actions";
    // 对齐 GitLens hover：uncommitted 不展示 copy/search/graph/remote；无 restore；有 previous 才有 prior。
    if (committed && short) {
      const hash = document.createElement("button");
      hash.type = "button";
      hash.className = "git-blame-card-hash";
      hash.title = t("scm.blame.copyHash");
      const gitIcon = document.createElement("span");
      gitIcon.className = "codicon codicon-git-commit";
      gitIcon.setAttribute("aria-hidden", "true");
      hash.append(gitIcon, document.createTextNode(` ${short}`));
      hash.addEventListener("mousedown", (event) => event.preventDefault());
      hash.addEventListener("click", (event) => {
        event.preventDefault();
        event.stopPropagation();
        this.fire("copy");
      });
      actions.append(hash);
      actions.append(this.sep());
      actions.append(this.iconButton("copy", "scm.blame.copyHash", "copy"));
      actions.append(this.iconButton("diff", "scm.blame.openChanges", "changes"));
      if (hasPrevious) {
        actions.append(this.iconButton("versions", "scm.blame.openPrior", "blame-prior"));
      }
      actions.append(this.sep());
      actions.append(this.iconButton("search", "scm.blame.openGraph", "graph"));
      if (remoteUrl) {
        actions.append(this.iconButton("globe", "scm.blame.openRemote", "remote"));
      }
    } else if (hasPrevious) {
      actions.append(this.iconButton("diff", "scm.blame.openChanges", "changes"));
      actions.append(this.iconButton("versions", "scm.blame.openPrior", "blame-prior"));
      actions.append(this.sep());
    }
    actions.append(this.iconButton("folder-opened", "scm.blame.reveal", "reveal"));
    actions.append(this.sep());

    const explain = document.createElement("button");
    explain.type = "button";
    explain.className = "git-blame-card-explain";
    explain.title = t("scm.blame.explain");
    const sparkle = document.createElement("span");
    sparkle.className = "codicon codicon-sparkle";
    sparkle.setAttribute("aria-hidden", "true");
    explain.append(sparkle, document.createTextNode(` ${t("scm.blame.explain")}`));
    explain.addEventListener("mousedown", (event) => event.preventDefault());
    explain.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      this.fire("explain");
    });
    actions.append(explain);

    const diffBox = document.createElement("div");
    diffBox.className = "git-blame-card-diff";
    if (diff.length === 0) {
      const empty = document.createElement("div");
      empty.className = "git-blame-card-diff-empty";
      empty.textContent = t("scm.blame.noDiff");
      diffBox.append(empty);
    } else {
      for (const row of diff) {
        const line = document.createElement("div");
        line.className = `git-blame-card-diff-line is-${row.kind}`;
        line.textContent = `${row.kind === "deleted" ? "-" : "+"}${row.text || " "}`;
        diffBox.append(line);
      }
    }

    root.append(head, message, actions, diffBox);

    if (committed && short) {
      const foot = document.createElement("div");
      foot.className = "git-blame-card-foot";
      const label = document.createElement("span");
      label.textContent = t("scm.blame.changes");
      const range = document.createElement("span");
      range.className = "git-blame-card-range";
      range.textContent = previous ? `${previous} → ${short}` : short;
      foot.append(label, range);
      root.append(foot);
    }
  }

  private sep(): HTMLElement {
    const el = document.createElement("span");
    el.className = "git-blame-card-sep";
    el.setAttribute("aria-hidden", "true");
    el.textContent = "|";
    return el;
  }

  private iconButton(icon: string, tipKey: string, action: BlameCardAction): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "git-blame-card-action";
    button.title = t(tipKey);
    const glyph = document.createElement("span");
    glyph.className = `codicon codicon-${icon}`;
    glyph.setAttribute("aria-hidden", "true");
    button.append(glyph);
    button.addEventListener("mousedown", (event) => event.preventDefault());
    button.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      this.fire(action);
    });
    return button;
  }

  private fire(action: BlameCardAction) {
    if (!this.model) return;
    this.onAction(action, this.model);
  }

  private reposition() {
    const root = this.root;
    const model = this.model;
    if (!root || !model) return;
    const layout = this.editor.getLayoutInfo();
    const width = Math.min(520, Math.max(280, layout.width - layout.contentLeft - 24));
    root.style.position = "absolute";
    root.style.width = `${width}px`;
    root.style.zIndex = "50";
    const cardHeight = root.offsetHeight || 160;
    const cardWidth = root.offsetWidth || width;
    // 同 VS Code preferAbove：行尾注解为锚，卡片左下角贴在该点（即显示在注解上方）。
    const anchor = this.anchor ?? fallbackAnchor(this.editor, model.line);
    let left = anchor.x;
    let top = anchor.y - cardHeight;
    // 上方不够则落到行下方（VS Code ABOVE→BELOW）。
    if (top < 8) top = anchor.y + (this.editor.getOption(monaco.editor.EditorOption.lineHeight) || 18);
    const maxLeft = Math.max(8, layout.width - cardWidth - 8);
    const maxTop = Math.max(8, layout.height - cardHeight - 8);
    left = Math.min(Math.max(8, left), maxLeft);
    top = Math.min(Math.max(8, top), maxTop);
    root.style.left = `${Math.round(left)}px`;
    root.style.top = `${Math.round(top)}px`;
  }
}

function fallbackAnchor(
  editor: Monaco.editor.IStandaloneCodeEditor,
  line: number,
): BlameCardAnchor {
  const model = editor.getModel();
  const column = model?.getLineMaxColumn(line) ?? 1;
  const pos = editor.getScrolledVisiblePosition({ lineNumber: line, column });
  return {
    x: pos?.left ?? Math.max(8, editor.getLayoutInfo().contentLeft),
    y: pos?.top ?? 8,
  };
}

function cardSignature(model: BlameCardModel): string {
  const diff = model.diff.map((row) => `${row.kind}:${row.text}`).join("\n");
  return [
    model.line,
    model.blame.hash,
    model.blame.committed ? "1" : "0",
    model.blame.previous ?? "",
    model.remoteUrl ?? "",
    diff,
  ].join("\0");
}
