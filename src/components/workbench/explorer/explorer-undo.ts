/** 资源管理器文件操作的撤销/重做栈（对应 VS Code UndoRedoService 的 explorer 来源）。 */
export interface UndoableFileOperation {
  label: string;
  /** 撤销会删掉文件（例如撤销新建/复制），按 confirmUndo=default 需要确认。 */
  destructiveUndo: boolean;
  undo(): Promise<void>;
  redo(): Promise<void>;
}

export class ExplorerUndoStack {
  private done: UndoableFileOperation[] = [];
  private undone: UndoableFileOperation[] = [];
  private busy = false;

  constructor(private readonly limit = 50) {}

  push(operation: UndoableFileOperation) {
    this.done.push(operation);
    if (this.done.length > this.limit) this.done.shift();
    this.undone = [];
  }

  get canUndo() {
    return !this.busy && this.done.length > 0;
  }

  get canRedo() {
    return !this.busy && this.undone.length > 0;
  }

  peekUndo(): UndoableFileOperation | undefined {
    return this.done[this.done.length - 1];
  }

  peekRedo(): UndoableFileOperation | undefined {
    return this.undone[this.undone.length - 1];
  }

  async undo(): Promise<UndoableFileOperation | undefined> {
    if (!this.canUndo) return undefined;
    const operation = this.done.pop()!;
    this.busy = true;
    try {
      await operation.undo();
      this.undone.push(operation);
      return operation;
    } catch (error) {
      // 撤销失败时整段历史已不可信（磁盘状态未知），清空避免越撤越乱。
      this.clear();
      throw error;
    } finally {
      this.busy = false;
    }
  }

  async redo(): Promise<UndoableFileOperation | undefined> {
    if (!this.canRedo) return undefined;
    const operation = this.undone.pop()!;
    this.busy = true;
    try {
      await operation.redo();
      this.done.push(operation);
      return operation;
    } catch (error) {
      this.clear();
      throw error;
    } finally {
      this.busy = false;
    }
  }

  clear() {
    this.done = [];
    this.undone = [];
  }
}
