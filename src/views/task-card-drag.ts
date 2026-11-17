/**
 * 看板卡片拖拽：用指针事件实现，不用 HTML5 拖放。
 * Tauri 开着 dragDropEnabled（接系统文件拖入）时，WebView 里的 HTML5 dragover / drop 不可靠，
 * 卡片拖不到别的列。文件树拖拽同理，见 components/chat/workspace-file-drag.ts。
 */

export const TASK_DRAG_START_DISTANCE = 5;
const EDGE_SIZE = 56;
const MAX_EDGE_SPEED = 18;

export interface TaskDropTarget {
  status: string;
  /** 插到这张卡片前面；空字符串表示列尾。 */
  beforeId: string;
}

export interface TaskCardRect {
  id: string;
  top: number;
  height: number;
}

/** 指针在列里的纵向位置 → 插在哪张卡片前面。被拖的卡片不参与计算。 */
export function dropBeforeId(cards: TaskCardRect[], y: number, draggingId: string): string {
  for (const card of cards) {
    if (card.id === draggingId) continue;
    if (y < card.top + card.height / 2) return card.id;
  }
  return "";
}

/** 指针下的列和插入位置。列靠 `data-status`，卡片靠 `data-task-id`。 */
export function dropTargetAt(x: number, y: number, draggingId: string): TaskDropTarget | null {
  const hit = document.elementFromPoint(x, y);
  const column = hit?.closest<HTMLElement>("[data-status]");
  const status = column?.dataset.status;
  if (!column || !status) return null;
  const cards = Array.from(column.querySelectorAll<HTMLElement>("[data-task-id]")).map((card) => {
    const rect = card.getBoundingClientRect();
    return { id: card.dataset.taskId ?? "", top: rect.top, height: rect.height };
  });
  return { status, beforeId: dropBeforeId(cards, y, draggingId) };
}

/** 指针贴近边缘时的滚动速度（像素/帧），负数往回滚。 */
export function edgeScrollSpeed(position: number, start: number, end: number): number {
  if (end - start <= EDGE_SIZE * 2) return 0;
  if (position < start + EDGE_SIZE) {
    return -Math.ceil(((start + EDGE_SIZE - position) / EDGE_SIZE) * MAX_EDGE_SPEED);
  }
  if (position > end - EDGE_SIZE) {
    return Math.ceil(((position - (end - EDGE_SIZE)) / EDGE_SIZE) * MAX_EDGE_SPEED);
  }
  return 0;
}

export interface TaskCardDragHandlers {
  onStart: () => void;
  onOver: (target: TaskDropTarget | null) => void;
  onDrop: (target: TaskDropTarget | null) => void;
  onCancel: () => void;
}

/**
 * 从卡片 pointerdown 开始跟踪；移动超过阈值才算拖动，否则松手照常触发 click。
 * 返回 true 表示已接管这次按下。
 */
export function beginTaskCardDrag(
  event: PointerEvent,
  taskId: string,
  board: HTMLElement | null,
  handlers: TaskCardDragHandlers,
): boolean {
  if (event.button !== 0 || activeSession) return false;
  const card = event.currentTarget as HTMLElement | null;
  if (!card) return false;
  const rect = card.getBoundingClientRect();
  const session: Session = {
    taskId,
    card,
    board,
    handlers,
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    offsetX: event.clientX - rect.left,
    offsetY: event.clientY - rect.top,
    width: rect.width,
    x: event.clientX,
    y: event.clientY,
    started: false,
    ghost: null,
    frame: 0,
  };
  try {
    card.setPointerCapture(event.pointerId);
  } catch {
    return false;
  }
  activeSession = session;
  card.addEventListener("pointermove", onMove);
  card.addEventListener("pointerup", onUp);
  card.addEventListener("pointercancel", onCancel);
  card.addEventListener("lostpointercapture", onCancel);
  window.addEventListener("keydown", onKeydown, true);
  return true;
}

export function cancelTaskCardDrag() {
  const session = activeSession;
  if (!session) return;
  const started = session.started;
  end(session);
  if (started) session.handlers.onCancel();
}

interface Session {
  taskId: string;
  card: HTMLElement;
  board: HTMLElement | null;
  handlers: TaskCardDragHandlers;
  pointerId: number;
  startX: number;
  startY: number;
  offsetX: number;
  offsetY: number;
  width: number;
  x: number;
  y: number;
  started: boolean;
  ghost: HTMLElement | null;
  frame: number;
}

let activeSession: Session | null = null;

function start(session: Session) {
  session.started = true;
  const ghost = session.card.cloneNode(true) as HTMLElement;
  ghost.classList.add("task-card-drag-ghost");
  ghost.classList.remove("is-dragging", "is-drop-before");
  ghost.style.width = `${session.width}px`;
  document.body.appendChild(ghost);
  session.ghost = ghost;
  document.body.classList.add("is-dragging-task");
  window.getSelection()?.removeAllRanges();
  session.handlers.onStart();
  tick(session);
}

function place(session: Session) {
  if (!session.ghost) return;
  session.ghost.style.transform =
    `translate(${session.x - session.offsetX}px, ${session.y - session.offsetY}px)`;
}

/** 每帧：跟随指针、贴边自动滚动、刷新落点。指针不动时滚动也要继续。 */
function tick(session: Session) {
  if (activeSession !== session || !session.started) return;
  place(session);
  const { board, x, y } = session;
  if (board) {
    const rect = board.getBoundingClientRect();
    const speed = edgeScrollSpeed(x, rect.left, rect.right);
    if (speed) board.scrollLeft += speed;
  }
  const list = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-status]")
    ?.querySelector<HTMLElement>("[data-task-list]");
  if (list) {
    const rect = list.getBoundingClientRect();
    const speed = edgeScrollSpeed(y, rect.top, rect.bottom);
    if (speed) list.scrollTop += speed;
  }
  session.handlers.onOver(dropTargetAt(x, y, session.taskId));
  session.frame = requestAnimationFrame(() => tick(session));
}

function onMove(event: PointerEvent) {
  const session = activeSession;
  if (!session || event.pointerId !== session.pointerId) return;
  session.x = event.clientX;
  session.y = event.clientY;
  if (session.started) return;
  const distance = Math.hypot(event.clientX - session.startX, event.clientY - session.startY);
  if (distance >= TASK_DRAG_START_DISTANCE) start(session);
}

function onUp(event: PointerEvent) {
  const session = activeSession;
  if (!session || event.pointerId !== session.pointerId) return;
  const started = session.started;
  const target = started ? dropTargetAt(event.clientX, event.clientY, session.taskId) : null;
  end(session);
  if (started) session.handlers.onDrop(target);
}

function onCancel(event: PointerEvent) {
  const session = activeSession;
  if (!session || event.pointerId !== session.pointerId) return;
  cancelTaskCardDrag();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape" || !activeSession?.started) return;
  event.preventDefault();
  event.stopPropagation();
  cancelTaskCardDrag();
}

function end(session: Session) {
  activeSession = null;
  cancelAnimationFrame(session.frame);
  const { card } = session;
  card.removeEventListener("pointermove", onMove);
  card.removeEventListener("pointerup", onUp);
  card.removeEventListener("pointercancel", onCancel);
  card.removeEventListener("lostpointercapture", onCancel);
  window.removeEventListener("keydown", onKeydown, true);
  if (card.hasPointerCapture?.(session.pointerId)) {
    card.releasePointerCapture(session.pointerId);
  }
  session.ghost?.remove();
  document.body.classList.remove("is-dragging-task");
}
