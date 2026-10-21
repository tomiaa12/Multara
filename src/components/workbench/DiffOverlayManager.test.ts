/**
 * @vitest-environment happy-dom
 */
import { describe, expect, test, vi } from "vitest";
import type { DiffChunk } from "../../domain/diff";
import { makeChunkId } from "../../domain/diff";

vi.mock("../../monaco", () => ({
  monaco: {
    editor: {
      ContentWidgetPositionPreference: { ABOVE: 0, BELOW: 1, EXACT: 2 },
      OverviewRulerLane: { Right: 2 },
      EditorOption: { fontInfo: 50 },
      EditorZoom: { onDidChangeZoomLevel: () => ({ dispose() {} }) },
      colorize: async (text: string) => `<span class="mtk1">${text}</span>`,
    },
  },
}));

import { DiffOverlayManager, groupAnchorLine, groupConsecutiveChunks, hunkTailLine } from "./DiffOverlayManager";

function chunk(index: number, newStart: number, newLines: string[], oldLines: string[] = []): DiffChunk {
  return {
    chunkId: makeChunkId("m1", "ws", "a.ts", index),
    messageId: "m1",
    workspaceId: "ws",
    filePath: "a.ts",
    chunkKey: String(index),
    status: "pending",
    hunk: { oldStart: newStart, oldLines, newStart, newLines },
  };
}

function fakeEditor() {
  const widgets: Array<{
    getDomNode: () => HTMLElement;
    getId: () => string;
    getPosition: () => null;
  }> = [];
  const zoneMap = new Map<string, unknown>();
  const zones: unknown[] = [];
  let nextZoneId = 1;
  const font = {
    fontFamily: "Consolas, monospace",
    fontSize: 13,
    lineHeight: 19,
    letterSpacing: 0,
    fontWeight: "normal",
  };
  const configListeners: Array<(event: { hasChanged: (option: number) => boolean }) => void> = [];
  function syncZones() {
    zones.length = 0;
    zones.push(...zoneMap.values());
  }
  return {
    widgets,
    zones,
    font,
    emitFontChange() {
      for (const listener of configListeners) {
        listener({ hasChanged: (option) => option === 50 });
      }
    },
    changeViewZones(fn: (accessor: { addZone: (zone: unknown) => string; removeZone: (id: string) => void }) => void) {
      fn({
        addZone: (zone) => {
          const id = String(nextZoneId++);
          zoneMap.set(id, zone);
          syncZones();
          return id;
        },
        removeZone: (id) => {
          zoneMap.delete(id);
          syncZones();
        },
      });
    },
    addOverlayWidget(widget: (typeof widgets)[number]) {
      widgets.push(widget);
    },
    removeOverlayWidget(widget: { getId: () => string }) {
      const index = widgets.findIndex((item) => item.getId() === widget.getId());
      if (index >= 0) widgets.splice(index, 1);
    },
    getTopForLineNumber(line: number) {
      return (line - 1) * font.lineHeight;
    },
    getScrollTop() {
      return 0;
    },
    onDidScrollChange() {
      return { dispose() {} };
    },
    onDidLayoutChange() {
      return { dispose() {} };
    },
    deltaDecorations() {
      return [];
    },
    onDidChangeConfiguration(listener: (event: { hasChanged: (option: number) => boolean }) => void) {
      configListeners.push(listener);
      return {
        dispose() {
          const index = configListeners.indexOf(listener);
          if (index >= 0) configListeners.splice(index, 1);
        },
      };
    },
    getOption() {
      return font;
    },
    getLayoutInfo() {
      return { contentLeft: 62 };
    },
    getModel() {
      return { getLanguageId: () => "plaintext", getLineCount: () => 80 };
    },
    getDomNode() {
      const host = document.createElement("div");
      host.style.color = "rgb(212, 212, 212)";
      const lines = document.createElement("div");
      lines.className = "view-lines";
      const line = document.createElement("div");
      line.className = "view-line";
      line.style.fontFamily = "Consolas, monospace";
      line.style.fontSize = "13px";
      line.style.lineHeight = "19px";
      line.style.letterSpacing = "0px";
      line.style.fontWeight = "400";
      line.style.whiteSpace = "pre";
      line.textContent = "sample";
      lines.append(line);
      host.append(lines);
      return host;
    },
  };
}

type Zone = { afterLineNumber: number; heightInPx?: number; heightInLines?: number; domNode: HTMLElement };

function controlWidgets(editor: ReturnType<typeof fakeEditor>) {
  return editor.widgets.filter((widget) => widget.getDomNode().querySelector(".diff-btn-accept"));
}

function removedZones(editor: ReturnType<typeof fakeEditor>): Zone[] {
  return (editor.zones as Zone[]).filter((zone) => zone.domNode.querySelector(".diff-removed-row"));
}

describe("DiffOverlayManager", () => {
  test("groups a delete+add pair and isolated hunks separately", () => {
    expect(hunkTailLine({ oldStart: 2, oldLines: ["B"], newStart: 2, newLines: ["X", "Y"] })).toBe(3);
    expect(hunkTailLine({ oldStart: 21, oldLines: ["removed"], newStart: 21, newLines: [] })).toBe(20);

    const modify = chunk(0, 2, ["X"], ["B"]);
    const next = chunk(1, 3, ["Y"], ["C"]);
    const isolated = chunk(2, 10, ["Z"], ["D"]);
    expect(groupConsecutiveChunks([modify, next, isolated])).toEqual([[modify, next], [isolated]]);
    expect(groupConsecutiveChunks([chunk(0, 5, [], ["gone"]), chunk(1, 5, ["added"])])).toHaveLength(1);

    const addedComment = chunk(0, 6, ["# xxxx"]);
    const priorLine = chunk(1, 6, ["# xxxx", "singleAttributePerLine: true"], ["singleAttributePerLine: true # note"]);
    expect(groupConsecutiveChunks([addedComment, priorLine])).toHaveLength(1);
    expect(groupAnchorLine([modify, next])).toBe(3);
    expect(groupAnchorLine([chunk(0, 21, [], ["removed"])])).toBe(21);
  });

  test("T-L01 shows Accept/Undo after the first edit and keeps them when the hunk grows", () => {
    const editor = fakeEditor();
    const onAccept = vi.fn();
    const onUndo = vi.fn();
    const overlay = new DiffOverlayManager(editor as never, onAccept, onUndo);

    overlay.applyChunks([chunk(0, 2, ["X"], ["B"])]);
    expect(controlWidgets(editor)).toHaveLength(1);
    const first = controlWidgets(editor)[0].getDomNode();
    expect(first.style.top).toBe("38px");
    expect(first.querySelector(".diff-chunk-controls-bar")).toBeTruthy();
    expect(first.querySelector(".diff-btn-accept")?.textContent).toBe("保留");
    expect(first.querySelector(".diff-btn-undo")?.textContent).toBe("撤销");

    overlay.applyChunks([chunk(0, 2, ["X", "Y"], ["B"])]);
    const second = controlWidgets(editor)[0].getDomNode();
    expect(second.style.top).toBe("57px");
    expect(second.querySelector(".diff-btn-accept")).toBeTruthy();
    expect(second.querySelector(".diff-btn-undo")).toBeTruthy();
    (second.querySelector(".diff-btn-undo") as HTMLButtonElement).click();
    expect(onUndo).toHaveBeenCalledWith(makeChunkId("m1", "ws", "a.ts", 0));
  });

  test("keeps Accept/Undo at the hunk when the hunk ends past the viewport", () => {
    const editor = fakeEditor();
    editor.getLayoutInfo = () => ({ contentLeft: 62, height: 40, verticalScrollbarWidth: 0 });
    const overlay = new DiffOverlayManager(editor as never, () => undefined, () => undefined);
    overlay.applyChunks([chunk(0, 2, ["new"], ["old"])]);
    expect(controlWidgets(editor)[0].getDomNode().style.top).toBe("38px");
  });

  test("Accept/Undo is positioned and does not insert editor height", () => {
    const editor = fakeEditor();
    const overlay = new DiffOverlayManager(editor as never, () => undefined, () => undefined);
    overlay.applyChunks([chunk(0, 2, ["new"], ["old"])]);
    expect(editor.zones.every((zone) => !(zone as Zone).domNode.querySelector(".diff-btn-accept"))).toBe(true);
    const el = controlWidgets(editor)[0].getDomNode();
    expect(el.style.position).toBe("absolute");
    expect(el.style.top).toBe("38px");
  });

  test("modify controls sit below the green line, not between red and green", () => {
    const editor = fakeEditor();
    const overlay = new DiffOverlayManager(editor as never, () => undefined, () => undefined);
    overlay.applyChunks([chunk(0, 2, ["new"], ["old"])]);
    expect(removedZones(editor)[0].afterLineNumber).toBe(1);
    expect(controlWidgets(editor)[0].getDomNode().style.top).toBe("38px");
  });

  test("consecutive -/+ hunks share one Accept/Undo at the group bottom", () => {
    const editor = fakeEditor();
    const onAccept = vi.fn();
    const overlay = new DiffOverlayManager(editor as never, onAccept, () => undefined);
    overlay.applyChunks([
      chunk(0, 2, ["X"], ["A"]),
      chunk(1, 3, ["Y"], ["B"]),
    ]);
    expect(controlWidgets(editor)).toHaveLength(1);
    expect(controlWidgets(editor)[0].getDomNode().style.top).toBe("57px");
    (controlWidgets(editor)[0].getDomNode().querySelector(".diff-btn-accept") as HTMLButtonElement).click();
    expect(onAccept).toHaveBeenCalledTimes(2);
    expect(onAccept).toHaveBeenNthCalledWith(1, makeChunkId("m1", "ws", "a.ts", 1));
    expect(onAccept).toHaveBeenNthCalledWith(2, makeChunkId("m1", "ws", "a.ts", 0));
  });

  test("acceptAll and undoAll trigger every visible control, undo from the bottom", async () => {
    const editor = fakeEditor();
    const onAccept = vi.fn();
    const onUndo = vi.fn();
    const overlay = new DiffOverlayManager(editor as never, onAccept, onUndo);
    overlay.applyChunks([
      chunk(0, 2, ["X"], ["A"]),
      chunk(1, 10, ["Y"], ["B"]),
    ]);
    expect(controlWidgets(editor)).toHaveLength(2);

    await overlay.acceptAll();
    expect(onAccept.mock.calls.map((call) => call[0])).toEqual([
      makeChunkId("m1", "ws", "a.ts", 0),
      makeChunkId("m1", "ws", "a.ts", 1),
    ]);

    await overlay.undoAll();
    expect(onUndo.mock.calls.map((call) => call[0])).toEqual([
      makeChunkId("m1", "ws", "a.ts", 1),
      makeChunkId("m1", "ws", "a.ts", 0),
    ]);
  });

  test("T-E08 applyChunks of 50 hunks finishes within 200ms", () => {
    const editor = fakeEditor();
    const overlay = new DiffOverlayManager(editor as never, () => undefined, () => undefined);
    const chunks = Array.from({ length: 50 }, (_, index) =>
      chunk(index, index * 3 + 1, [`new-${index}`], [`old-${index}`]),
    );
    const started = performance.now();
    overlay.applyChunks(chunks);
    expect(performance.now() - started).toBeLessThan(200);
    expect(controlWidgets(editor)).toHaveLength(50);
    expect(removedZones(editor)).toHaveLength(50);
  });

  test("delete-only controls sit below the red zone instead of the next line", () => {
    const editor = fakeEditor();
    const overlay = new DiffOverlayManager(editor as never, () => undefined, () => undefined);
    overlay.applyChunks([chunk(0, 21, [], ["removed"])]);
    expect(removedZones(editor)).toHaveLength(1);
    expect(controlWidgets(editor)).toHaveLength(1);
    expect(removedZones(editor)[0].afterLineNumber).toBe(20);
    expect(controlWidgets(editor)[0].getDomNode().style.top).toBe("380px");
    const widget = controlWidgets(editor)[0].getDomNode();
    expect(widget.querySelector(".diff-btn-accept")).toBeTruthy();
    expect(widget.querySelector(".diff-btn-undo")).toBeTruthy();
    const zone = removedZones(editor)[0];
    const row = zone.domNode.querySelector(".diff-removed-row") as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.querySelector(".diff-prefix")).toBeNull();
    expect(row.textContent).toBe("removed");
    expect(row.style.fontFamily).toContain("Consolas");
    expect(row.style.fontSize).toBe("13px");
    expect(row.style.lineHeight).toBe("19px");
    expect(row.style.paddingLeft).toBe("");
    expect(zone.domNode.style.paddingLeft).toBe("");
  });

  test("T-RT03 disables Accept/Undo while running", () => {
    const editor = fakeEditor();
    const onAccept = vi.fn();
    const overlay = new DiffOverlayManager(editor as never, onAccept, () => undefined, () => true);
    overlay.applyChunks([chunk(0, 2, ["X"], ["B"])]);
    const host = controlWidgets(editor)[0].getDomNode();
    const accept = host.querySelector(".diff-btn-accept") as HTMLButtonElement;
    const undo = host.querySelector(".diff-btn-undo") as HTMLButtonElement;
    expect(accept.disabled).toBe(true);
    expect(undo.disabled).toBe(true);
    expect(accept.title).toBe("正在修改此文件");
    expect(host.className).toContain("diff-controls-disabled");
    accept.click();
    expect(onAccept).not.toHaveBeenCalled();
  });

  test("T-RT04 enables Accept/Undo after the run completes", () => {
    const editor = fakeEditor();
    let running = true;
    const onAccept = vi.fn();
    const overlay = new DiffOverlayManager(editor as never, onAccept, () => undefined, () => running);
    overlay.applyChunks([chunk(0, 2, ["X"], ["B"])]);
    expect((controlWidgets(editor)[0].getDomNode().querySelector(".diff-btn-accept") as HTMLButtonElement).disabled).toBe(true);
    running = false;
    overlay.applyChunks([chunk(0, 2, ["X"], ["B"])]);
    const accept = controlWidgets(editor)[0].getDomNode().querySelector(".diff-btn-accept") as HTMLButtonElement;
    expect(accept.disabled).toBe(false);
    accept.click();
    expect(onAccept).toHaveBeenCalledWith(makeChunkId("m1", "ws", "a.ts", 0));
  });

  test("multi-line removed rows stack instead of overlapping", () => {
    const editor = fakeEditor();
    const overlay = new DiffOverlayManager(editor as never, () => undefined, () => undefined);
    overlay.applyChunks([chunk(0, 5, [], ["one", "two", "three"])]);
    const rows = [...removedZones(editor)[0].domNode.querySelectorAll(".diff-removed-row")] as HTMLElement[];
    expect(rows).toHaveLength(3);
    expect(rows.every((row) => !row.classList.contains("view-line"))).toBe(true);
    expect(rows.every((row) => row.style.position === "relative")).toBe(true);
    expect(rows.every((row) => row.style.height === "19px")).toBe(true);
  });

  test("removed zone follows editor zoom font metrics", () => {
    const editor = fakeEditor();
    const overlay = new DiffOverlayManager(editor as never, () => undefined, () => undefined);
    overlay.applyChunks([chunk(0, 2, ["X"], ["old"])]);
    const before = removedZones(editor)[0].domNode.querySelector(".diff-removed-row") as HTMLElement;
    expect(before.style.fontSize).toBe("13px");
    expect(before.style.lineHeight).toBe("19px");
    expect(before.style.height).toBe("19px");
    expect(controlWidgets(editor)[0].getDomNode().style.top).toBe("38px");

    editor.font.fontSize = 20;
    editor.font.lineHeight = 28;
    editor.emitFontChange();

    const after = removedZones(editor)[0].domNode.querySelector(".diff-removed-row") as HTMLElement;
    expect(after.style.fontSize).toBe("20px");
    expect(after.style.lineHeight).toBe("28px");
    expect(after.style.height).toBe("28px");
    expect(controlWidgets(editor)[0].getDomNode().style.top).toBe("56px");
    overlay.dispose();
  });
});
