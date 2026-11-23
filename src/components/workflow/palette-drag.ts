/**
 * 工作流节点面板拖拽：用指针事件实现，不用 HTML5 拖放。
 * Tauri 开着 dragDropEnabled（接系统文件拖入）时，macOS WKWebView 里的
 * HTML5 dragover / drop 不可靠——网页端能拖、桌面端拖不进。
 * 与任务看板（task-card-drag）/ 文件树（workspace-file-drag）同一策略。
 */

export type PalettePayload =
  | { type: "kind"; kind: string }
  | { type: "template"; id: string }
  | { type: "agent"; agentType: "agent" | "squad"; id: string };

export const WORKFLOW_DROP_ATTR = "data-workflow-drop";
export const PALETTE_DRAG_START_DISTANCE = 5;

export type PaletteDropPoint = { x: number; y: number };

export interface WorkflowPaletteDragHandlers {
  /** 拖影上的文字 */
  label: string;
  onDrop: (payload: PalettePayload, point: PaletteDropPoint | null) => void;
  onCancel?: () => void;
}

interface Session {
  payload: PalettePayload;
  target: HTMLElement;
  handlers: WorkflowPaletteDragHandlers;
  pointerId: number;
  startX: number;
  startY: number;
  x: number;
  y: number;
  started: boolean;
  ghost: HTMLDivElement | null;
}

let activeSession: Session | null = null;

/** 指针下是否落在画布投放区（含画布内的节点/空白）。 */
export function workflowDropPointAt(x: number, y: number): PaletteDropPoint | null {
  const hit = document.elementFromPoint(x, y);
  if (!hit?.closest(`[${WORKFLOW_DROP_ATTR}]`)) return null;
  return { x, y };
}

export function beginWorkflowPalettePointerDrag(
  event: PointerEvent,
  payload: PalettePayload,
  handlers: WorkflowPaletteDragHandlers,
): boolean {
  if (event.button !== 0 || activeSession) return false;
  const target = event.currentTarget as HTMLElement | null;
  if (!target) return false;
  const session: Session = {
    payload,
    target,
    handlers,
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    x: event.clientX,
    y: event.clientY,
    started: false,
    ghost: null,
  };
  try {
    target.setPointerCapture(event.pointerId);
  } catch {
    return false;
  }
  activeSession = session;
  target.addEventListener("pointermove", onMove);
  target.addEventListener("pointerup", onUp);
  target.addEventListener("pointercancel", onCancel);
  target.addEventListener("lostpointercapture", onCancel);
  window.addEventListener("keydown", onKeydown, true);
  return true;
}

export function cancelWorkflowPaletteDrag() {
  const session = activeSession;
  if (!session) return;
  const started = session.started;
  end(session);
  if (started) session.handlers.onCancel?.();
}

function start(session: Session) {
  session.started = true;
  const ghost = document.createElement("div");
  ghost.className = "workflow-palette-drag-ghost";
  ghost.textContent = session.handlers.label;
  document.body.appendChild(ghost);
  session.ghost = ghost;
  document.body.classList.add("is-dragging-workflow-palette");
  window.getSelection()?.removeAllRanges();
  place(session);
}

function place(session: Session) {
  if (!session.ghost) return;
  session.ghost.style.transform = `translate(${session.x + 12}px, ${session.y + 12}px)`;
}

function onMove(event: PointerEvent) {
  const session = activeSession;
  if (!session || event.pointerId !== session.pointerId) return;
  session.x = event.clientX;
  session.y = event.clientY;
  if (session.started) {
    place(session);
    return;
  }
  const distance = Math.hypot(event.clientX - session.startX, event.clientY - session.startY);
  if (distance >= PALETTE_DRAG_START_DISTANCE) start(session);
}

function onUp(event: PointerEvent) {
  const session = activeSession;
  if (!session || event.pointerId !== session.pointerId) return;
  const started = session.started;
  const point = started ? workflowDropPointAt(event.clientX, event.clientY) : null;
  const { payload, handlers } = session;
  end(session);
  if (started) handlers.onDrop(payload, point);
}

function onCancel(event: PointerEvent) {
  const session = activeSession;
  if (!session || event.pointerId !== session.pointerId) return;
  cancelWorkflowPaletteDrag();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape" || !activeSession?.started) return;
  event.preventDefault();
  event.stopPropagation();
  cancelWorkflowPaletteDrag();
}

function end(session: Session) {
  activeSession = null;
  const { target } = session;
  target.removeEventListener("pointermove", onMove);
  target.removeEventListener("pointerup", onUp);
  target.removeEventListener("pointercancel", onCancel);
  target.removeEventListener("lostpointercapture", onCancel);
  window.removeEventListener("keydown", onKeydown, true);
  if (target.hasPointerCapture?.(session.pointerId)) {
    try {
      target.releasePointerCapture(session.pointerId);
    } catch {
      // already released
    }
  }
  session.ghost?.remove();
  document.body.classList.remove("is-dragging-workflow-palette");
}
