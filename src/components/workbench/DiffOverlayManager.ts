import type * as MonacoNS from "monaco-editor";
import { monaco } from "../../monaco";
import type { DiffChunk, DiffHunk } from "../../domain/diff";

const RUNNING_HINT = "正在修改此文件";
const CONTROLS_RIGHT_GAP = 8;
/** 该 hunk 在当前文件中的最后一行（删除-only 为红块所挂的那一行） */
export function hunkTailLine(hunk: DiffHunk): number {
  if (hunk.newLines.length > 0) {
    return hunk.newStart + hunk.newLines.length - 1;
  }
  return hunk.newStart - 1;
}

export function groupVisualEnd(group: DiffChunk[]): number {
  return Math.max(...group.map((chunk) => hunkTailLine(chunk.hunk)));
}

/** 一组 Accept/Undo 在编辑器里应对齐到视口中间的行。 */
export function groupAnchorLine(group: DiffChunk[]): number {
  const lines = group.flatMap((chunk) => [
    Math.max(1, chunk.hunk.newStart),
    Math.max(1, hunkTailLine(chunk.hunk)),
  ]);
  const top = Math.min(...lines);
  const bottom = Math.max(...lines);
  return Math.max(1, Math.round((top + bottom) / 2));
}

/** 紧邻或重叠的 -, +, -/+（含跨轮）合成一组，共用一个 Accept/Undo */
export function groupConsecutiveChunks(chunks: DiffChunk[]): DiffChunk[][] {
  if (chunks.length === 0) return [];
  const sorted = [...chunks].sort((left, right) => {
    const byStart = left.hunk.newStart - right.hunk.newStart;
    if (byStart !== 0) return byStart;
    return left.chunkKey.localeCompare(right.chunkKey);
  });
  const groups: DiffChunk[][] = [[sorted[0]]];
  for (const chunk of sorted.slice(1)) {
    const group = groups[groups.length - 1];
    if (chunk.hunk.newStart <= groupVisualEnd(group) + 1) {
      group.push(chunk);
    } else {
      groups.push([chunk]);
    }
  }
  return groups;
}

export class DiffOverlayManager {
  private decorationIds: string[] = [];
  private viewZoneIds: string[] = [];
  private overlayWidgets: MonacoNS.editor.IOverlayWidget[] = [];
  private controlPins: Array<{ el: HTMLElement; group: DiffChunk[] }> = [];
  private lastChunks: DiffChunk[] = [];
  private disposables: Array<{ dispose: () => void }> = [];
  private refreshingMetrics = false;
  private appliedSignature = "";

  constructor(
    private editor: MonacoNS.editor.IStandaloneCodeEditor,
  private onAccept: (chunkId: string) => void | Promise<void>,
  private onUndo: (chunkId: string) => void | Promise<void>,
    private isRunning: () => boolean = () => false,
  ) {
    this.bindMetricListeners();
  }

  applyChunks(chunks: DiffChunk[]): void {
    this.lastChunks = chunks;
    // chunk 没变就不要拆装 view zone / overlay widget：每次重建都会让 Monaco 重新布局。
    const signature = `${this.isRunning() ? 1 : 0}|${chunks.map((chunk) =>
      `${chunk.chunkId}:${chunk.hunk.newStart}:${chunk.hunk.oldLines.length}:${chunk.hunk.newLines.length}`,
    ).join(";")}`;
    if (signature === this.appliedSignature && this.decorationIds.length + this.viewZoneIds.length + this.overlayWidgets.length > 0) {
      this.repositionControls();
      return;
    }
    this.clearAll();
    this.appliedSignature = signature;

    const newDecorations: MonacoNS.editor.IModelDeltaDecoration[] = [];
    const disabled = this.isRunning();
    const groups = groupConsecutiveChunks(chunks);

    this.editor.changeViewZones((accessor) => {
      for (const chunk of chunks) {
        const { hunk } = chunk;

        if (hunk.oldLines.length > 0) {
          const domNode = this.buildRemovedZone(hunk.oldLines);
          const zoneId = accessor.addZone({
            afterLineNumber: hunk.newStart - 1,
            heightInLines: hunk.oldLines.length,
            domNode,
            suppressMouseDown: true,
          });
          this.viewZoneIds.push(zoneId);
        }

        if (hunk.newLines.length > 0) {
          const overview = hunk.oldLines.length === 0
            ? "rgba(72, 126, 2, 0.6)"
            : "rgba(27, 129, 168, 0.6)";
          newDecorations.push({
            range: {
              startLineNumber: hunk.newStart,
              endLineNumber: hunk.newStart + hunk.newLines.length - 1,
              startColumn: 1,
              endColumn: 1,
            },
            options: {
              isWholeLine: true,
              className: "diff-line-added",
              overviewRuler: {
                color: overview,
                position: monaco.editor.OverviewRulerLane.Left,
              },
            },
          });
        } else if (hunk.oldLines.length > 0) {
          const lineCount = this.editor.getModel()?.getLineCount() ?? 0;
          const anchor = Math.min(lineCount, Math.max(1, hunk.newStart || 1));
          if (lineCount >= 1) {
            newDecorations.push({
              range: {
                startLineNumber: anchor,
                endLineNumber: anchor,
                startColumn: 1,
                endColumn: 1,
              },
              options: {
                overviewRuler: {
                  color: "rgba(241, 76, 76, 0.6)",
                  position: monaco.editor.OverviewRulerLane.Left,
                },
              },
            });
          }
        }
      }
    });

    for (const group of groups) {
      this.addGroupControls(group, disabled);
    }

    this.decorationIds = this.editor.deltaDecorations([], newDecorations);
  }

  pendingChunks(): DiffChunk[] {
    return this.lastChunks.slice();
  }

  /** 与当前页每一组 Accept 按钮相同：按已画出来的 chunk 逐个接受。 */
  async acceptAll(): Promise<void> {
    if (this.isRunning()) return;
    const groups = groupConsecutiveChunks(this.lastChunks);
    for (const group of groups) {
      for (const chunk of group) await this.onAccept(chunk.chunkId);
    }
  }

  /** 与当前页每一组 Undo 相同，从文件末尾的那组开始。 */
  async undoAll(): Promise<void> {
    if (this.isRunning()) return;
    const groups = [...groupConsecutiveChunks(this.lastChunks)].reverse();
    for (const group of groups) await this.undoGroup(group);
  }

  private bindMetricListeners() {
    const config = this.editor.onDidChangeConfiguration?.((event) => {
      const fontOpt = monaco.editor.EditorOption?.fontInfo;
      if (event && fontOpt != null && event.hasChanged && !event.hasChanged(fontOpt)) return;
      this.refreshMetrics();
    });
    if (config) this.disposables.push(config);

    const zoom = monaco.editor.EditorZoom?.onDidChangeZoomLevel?.(() => this.refreshMetrics());
    if (zoom) this.disposables.push(zoom);

    const scroll = this.editor.onDidScrollChange?.(() => this.repositionControls());
    if (scroll) this.disposables.push(scroll);
    const layout = this.editor.onDidLayoutChange?.(() => this.repositionControls());
    if (layout) this.disposables.push(layout);
  }

  private refreshMetrics() {
    if (this.refreshingMetrics || this.lastChunks.length === 0) return;
    this.refreshingMetrics = true;
    this.appliedSignature = "";
    try {
      this.applyChunks(this.lastChunks);
    } finally {
      this.refreshingMetrics = false;
    }
  }

  private editorFontInfo() {
    const option = monaco.editor.EditorOption?.fontInfo;
    return option == null ? undefined : this.editor.getOption?.(option);
  }

  private liveViewLine(): HTMLElement | null {
    const host = this.editor.getDomNode?.();
    return (
      host?.querySelector(".view-lines .view-line:not(.diff-removed-row)") ??
      host?.querySelector(".view-line:not(.diff-removed-row)") ??
      null
    );
  }

  /** ViewZone 已在 .lines-content 内（行号右侧）。字号跟 editor fontInfo（含缩放），不垫 contentLeft。 */
  private copyEditorType(target: HTMLElement): void {
    const font = this.editorFontInfo();
    const sample = this.liveViewLine();
    if (font) {
      target.style.fontFamily = font.fontFamily;
      target.style.fontSize = `${font.fontSize}px`;
      target.style.lineHeight = `${font.lineHeight}px`;
      target.style.letterSpacing = `${font.letterSpacing}px`;
      target.style.fontWeight = String(font.fontWeight);
      target.style.whiteSpace = "pre";
      target.style.height = `${font.lineHeight}px`;
    } else if (sample) {
      const style = getComputedStyle(sample);
      target.style.fontFamily = style.fontFamily || sample.style.fontFamily;
      target.style.fontSize = style.fontSize || sample.style.fontSize;
      target.style.lineHeight = style.lineHeight || sample.style.lineHeight;
      target.style.letterSpacing = style.letterSpacing || sample.style.letterSpacing;
      target.style.fontWeight = style.fontWeight || sample.style.fontWeight;
      target.style.whiteSpace = style.whiteSpace === "normal" ? "pre" : style.whiteSpace || "pre";
    }
    const host = this.editor.getDomNode?.();
    if (host) {
      target.style.color = getComputedStyle(host).color;
    }
  }

  private paintRemovedLine(row: HTMLElement, line: string): void {
    const text = line || "\u00a0";
    const inner = document.createElement("span");
    inner.textContent = text;
    row.replaceChildren(inner);

    const language = this.editor.getModel?.()?.getLanguageId?.() ?? "plaintext";
    void monaco.editor.colorize?.(text, language, {}).then((html) => {
      if (!row.isConnected) return;
      inner.innerHTML = html.replace(/<br\s*\/?>/gi, "");
    });
  }

  private buildRemovedZone(oldLines: string[]): HTMLElement {
    const container = document.createElement("div");
    container.className = "diff-removed-zone";
    this.copyEditorType(container);
    container.style.height = "";
    for (const line of oldLines) {
      const row = document.createElement("div");
      row.className = "diff-removed-row";
      this.copyEditorType(row);
      row.style.position = "relative";
      row.style.top = "auto";
      row.style.left = "auto";
      this.paintRemovedLine(row, line);
      container.append(row);
    }
    return container;
  }

  private lineHeight(): number {
    return this.editorFontInfo()?.lineHeight ?? 19;
  }

  private hunkBottomTop(hunk: DiffHunk): number {
    const lineHeight = this.lineHeight();
    if (hunk.newLines.length > 0) {
      const line = hunk.newStart + hunk.newLines.length - 1;
      return (this.editor.getTopForLineNumber?.(line) ?? (line - 1) * lineHeight) + lineHeight;
    }
    const nextLine = hunk.newStart;
    const lineCount = this.editor.getModel?.()?.getLineCount?.() ?? 0;
    if (nextLine >= 1 && nextLine <= lineCount) {
      return this.editor.getTopForLineNumber?.(nextLine) ?? nextLine * lineHeight;
    }
    const lastLine = Math.max(1, lineCount);
    const lastTop = this.editor.getTopForLineNumber?.(lastLine) ?? (lastLine - 1) * lineHeight;
    return lastTop + lineHeight + hunk.oldLines.length * lineHeight;
  }

  /** 整组视觉底部（文档坐标），按钮 top 从这里起，不占编辑区高度 */
  private groupBottomTop(group: DiffChunk[]): number {
    return Math.max(...group.map((chunk) => this.hunkBottomTop(chunk.hunk)));
  }

  private positionControls(el: HTMLElement, group: DiffChunk[]) {
    const layout = this.editor.getLayoutInfo?.();
    const top = this.groupBottomTop(group) - (this.editor.getScrollTop?.() ?? 0);
    const right = (layout?.verticalScrollbarWidth ?? 0) + CONTROLS_RIGHT_GAP;
    el.style.position = "absolute";
    el.style.top = `${Math.round(top)}px`;
    el.style.right = `${right}px`;
    el.style.left = "auto";
  }

  private repositionControls() {
    for (const pin of this.controlPins) {
      this.positionControls(pin.el, pin.group);
    }
  }

  private addGroupControls(group: DiffChunk[], disabled: boolean) {
    const el = this.buildControls(group, disabled);
    const widget: MonacoNS.editor.IOverlayWidget = {
      getId: () => `diff-overlay-${group.map((chunk) => chunk.chunkId).join("+")}`,
      getDomNode: () => el,
      getPosition: () => null,
    };
    this.editor.addOverlayWidget(widget);
    this.overlayWidgets.push(widget);
    this.controlPins.push({ el, group });
    this.positionControls(el, group);
  }

  private buildControls(group: DiffChunk[], disabled: boolean): HTMLElement {
    const el = document.createElement("div");
    el.className = `diff-chunk-controls${disabled ? " diff-controls-disabled" : ""}`;

    const acceptBtn = this.makeButton("diff-btn-accept", "保留", disabled, () => {
      const ordered = [...group].sort((left, right) => right.hunk.oldStart - left.hunk.oldStart);
      for (const chunk of ordered) void this.onAccept(chunk.chunkId);
    });
    const undoBtn = this.makeButton("diff-btn-undo", "撤销", disabled, () => {
      void this.undoGroup(group);
    });

    const bar = document.createElement("div");
    bar.className = "diff-chunk-controls-bar";
    bar.append(acceptBtn, undoBtn);
    el.append(bar);
    return el;
  }

  private async undoGroup(group: DiffChunk[]): Promise<void> {
    if (this.isRunning()) return;
    for (const chunk of [...group].reverse()) {
      await this.onUndo(chunk.chunkId);
    }
  }

  private makeButton(
    cls: string,
    text: string,
    disabled: boolean,
    onClick: () => void,
  ): HTMLButtonElement {
    const btn = document.createElement("button");
    btn.className = `diff-btn ${cls}`;
    btn.textContent = text;
    btn.disabled = disabled;
    if (disabled) btn.title = RUNNING_HINT;
    btn.addEventListener("click", (event) => {
      event.stopPropagation();
      if (this.isRunning() || btn.disabled) return;
      onClick();
    });
    return btn;
  }

  clearAll(): void {
    this.appliedSignature = "";
    this.decorationIds = this.editor.deltaDecorations(this.decorationIds, []);
    this.editor.changeViewZones((accessor) => {
      this.viewZoneIds.forEach((id) => accessor.removeZone(id));
    });
    this.viewZoneIds = [];
    this.overlayWidgets.forEach((widget) => this.editor.removeOverlayWidget(widget));
    this.overlayWidgets = [];
    this.controlPins = [];
  }

  dispose(): void {
    this.disposables.forEach((item) => item.dispose());
    this.disposables = [];
    this.lastChunks = [];
    this.clearAll();
  }
}
