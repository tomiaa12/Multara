import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import { computed, ref } from "vue";

const STORAGE_KEY = "multara.editor.zoom";
const MIN_ZOOM = -5;
const MAX_ZOOM = 20;
const HUD_HIDE_MS = 2000;

export const monacoZoomOptions = {
  mouseWheelZoom: true,
  fontSize: 13,
} as const;

export const editorZoomLevel = ref(0);
export const editorZoomHudVisible = ref(false);

export const editorZoomPercent = computed(() => Math.round(1.2 ** editorZoomLevel.value * 100));
export const editorZoomLabel = computed(() => `${editorZoomPercent.value}%`);
export const canZoomIn = computed(() => editorZoomLevel.value < MAX_ZOOM);
export const canZoomOut = computed(() => editorZoomLevel.value > MIN_ZOOM);

let zoomApi: typeof Monaco = monaco;
let hideHudTimer = 0;
let listenersBound = false;
let restoring = false;

function zoom() {
  return zoomApi.editor.EditorZoom;
}

function clampZoom(level: number) {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(level)));
}

function readStoredZoom() {
  const saved = Number(localStorage.getItem(STORAGE_KEY));
  return Number.isFinite(saved) ? clampZoom(saved) : 0;
}

function persistLevel(level: number) {
  editorZoomLevel.value = clampZoom(level);
  localStorage.setItem(STORAGE_KEY, String(editorZoomLevel.value));
}

function revealZoomHud() {
  editorZoomHudVisible.value = true;
  window.clearTimeout(hideHudTimer);
  hideHudTimer = window.setTimeout(() => {
    editorZoomHudVisible.value = false;
  }, HUD_HIDE_MS);
}

function applyZoom(level: number, showHud: boolean) {
  const next = clampZoom(level);
  restoring = !showHud;
  zoom().setZoomLevel(next);
  persistLevel(zoom().getZoomLevel());
  restoring = false;
  if (showHud) revealZoomHud();
}

export function zoomEditorIn() {
  applyZoom(zoom().getZoomLevel() + 1, true);
}

export function zoomEditorOut() {
  applyZoom(zoom().getZoomLevel() - 1, true);
}

export function resetEditorZoom() {
  applyZoom(0, true);
}

export function bindEditorZoom(editor: Monaco.editor.IStandaloneCodeEditor, api: typeof Monaco = monaco) {
  zoomApi = api;
  bindZoomSideEffects(api);
  restoring = true;
  zoom().setZoomLevel(readStoredZoom());
  persistLevel(zoom().getZoomLevel());
  restoring = false;
  editor.updateOptions({ mouseWheelZoom: true });
  const disposables = [
    editor.addAction({
      id: "multara-font-zoom-in",
      label: "放大编辑器",
      keybindings: [
        api.KeyMod.CtrlCmd | api.KeyCode.Equal,
        api.KeyMod.CtrlCmd | api.KeyMod.Shift | api.KeyCode.Equal,
        api.KeyMod.CtrlCmd | api.KeyCode.NumpadAdd,
      ],
      run: () => zoomEditorIn(),
    }),
    editor.addAction({
      id: "multara-font-zoom-out",
      label: "缩小编辑器",
      keybindings: [
        api.KeyMod.CtrlCmd | api.KeyCode.Minus,
        api.KeyMod.CtrlCmd | api.KeyCode.NumpadSubtract,
      ],
      run: () => zoomEditorOut(),
    }),
    editor.addAction({
      id: "multara-font-zoom-reset",
      label: "复位编辑器缩放",
      keybindings: [
        api.KeyMod.CtrlCmd | api.KeyCode.Digit0,
        api.KeyMod.CtrlCmd | api.KeyCode.Numpad0,
      ],
      run: () => resetEditorZoom(),
    }),
  ];
  return {
    dispose() {
      disposables.forEach((item) => item.dispose());
    },
  };
}

function bindZoomSideEffects(api: typeof Monaco) {
  if (listenersBound) return;
  listenersBound = true;
  api.editor.EditorZoom.onDidChangeZoomLevel((level) => {
    persistLevel(typeof level === "number" ? level : api.editor.EditorZoom.getZoomLevel());
    if (!restoring) revealZoomHud();
  });
  window.addEventListener(
    "wheel",
    (event) => {
      if (!event.ctrlKey && !event.metaKey) return;
      if (event.target instanceof Element && event.target.closest(".monaco-editor, .monaco-host, .editor-area")) {
        return;
      }
      event.preventDefault();
    },
    { passive: false, capture: true },
  );
}
