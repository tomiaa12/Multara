import type { IBufferLine, IDisposable, IMarker, Terminal } from "@xterm/xterm";

/** VS Code 的 OSC 633 序列：A 提示符开始、B 输入开始、C 命令执行、D 命令结束、P 属性。 */
export type ShellSequence =
  | { kind: "promptStart" }
  | { kind: "commandStart" }
  | { kind: "executed" }
  | { kind: "finished"; exitCode?: number }
  | { kind: "cwd"; cwd: string };

export function parseOsc633(data: string): ShellSequence | null {
  const separator = data.indexOf(";");
  const head = separator < 0 ? data : data.slice(0, separator);
  const rest = separator < 0 ? "" : data.slice(separator + 1);
  if (head === "A") return { kind: "promptStart" };
  if (head === "B") return { kind: "commandStart" };
  if (head === "C") return { kind: "executed" };
  if (head === "D") {
    const code = rest === "" ? Number.NaN : Number(rest);
    return Number.isInteger(code) ? { kind: "finished", exitCode: code } : { kind: "finished" };
  }
  if (head === "P") {
    const equals = rest.indexOf("=");
    if (equals > 0 && rest.slice(0, equals) === "Cwd") {
      const cwd = rest.slice(equals + 1).trim();
      return cwd ? { kind: "cwd", cwd } : null;
    }
  }
  return null;
}

/** OSC 7：`file://host/path`。Windows 下去掉盘符前的斜杠。 */
export function parseOsc7(data: string): string | null {
  if (!data.startsWith("file://")) return null;
  const withoutScheme = data.slice("file://".length);
  const slash = withoutScheme.indexOf("/");
  if (slash < 0) return null;
  let path: string;
  try {
    path = decodeURIComponent(withoutScheme.slice(slash));
  } catch {
    return null;
  }
  if (/^\/[A-Za-z]:[\\/]/.test(path)) path = path.slice(1);
  return path || null;
}

export interface InputRow {
  text: string;
  /** 这一行是上一行自动折行出来的。 */
  wrapped: boolean;
}

/** 把输入区域的若干行拼成命令文本：折行直接拼接，真换行保留换行。 */
export function joinInputRows(rows: InputRow[], startX: number) {
  let text = "";
  rows.forEach((row, index) => {
    const part = index === 0 ? row.text.slice(startX) : row.text;
    if (index > 0 && !row.wrapped) text += "\n";
    text += part;
  });
  return text.trim();
}

export interface TerminalCommand {
  id: number;
  promptMarker: IMarker;
  inputMarker?: IMarker;
  inputX: number;
  executedMarker?: IMarker;
  endMarker?: IMarker;
  command: string;
  cwd: string;
  exitCode?: number;
  startedAt?: number;
  finishedAt?: number;
}

export interface ShellIntegrationHandlers {
  onFinished?: (command: TerminalCommand) => void;
  onCwd?: (cwd: string) => void;
  onDetected?: () => void;
}

const COMMAND_LIMIT = 1000;

export class ShellIntegration implements IDisposable {
  readonly commands: TerminalCommand[] = [];
  cwd: string;
  detected = false;
  private current: TerminalCommand | null = null;
  private state: "idle" | "prompt" | "input" | "executing" = "idle";
  /** 一旦见过 C，就说明 shell 会报告执行时机，不再用输入行推断命令。 */
  private reportsExecution = false;
  private nextId = 1;
  private readonly disposables: IDisposable[] = [];

  constructor(
    private readonly term: Terminal,
    cwd: string,
    private readonly handlers: ShellIntegrationHandlers = {},
  ) {
    this.cwd = cwd;
    this.disposables.push(
      term.parser.registerOscHandler(633, (data) => {
        const sequence = parseOsc633(data);
        if (sequence) this.handle(sequence);
        return true;
      }),
      term.parser.registerOscHandler(7, (data) => {
        const cwd = parseOsc7(data);
        if (cwd) this.setCwd(cwd);
        return false;
      }),
    );
  }

  dispose() {
    for (const item of this.disposables) item.dispose();
    this.disposables.length = 0;
  }

  /** 当前正在输入的命令起点（绝对行号与列），不在输入状态时为 null。 */
  inputStart(): { row: number; x: number } | null {
    const marker = this.current?.inputMarker;
    if (this.state !== "input" || !marker || marker.isDisposed || marker.line < 0) return null;
    return { row: marker.line, x: this.current!.inputX };
  }

  isExecuting() {
    return this.state === "executing";
  }

  liveCommands() {
    return this.commands.filter((item) => !item.promptMarker.isDisposed && item.promptMarker.line >= 0);
  }

  commandAtLine(row: number) {
    const list = this.liveCommands();
    for (let index = list.length - 1; index >= 0; index -= 1) {
      const item = list[index]!;
      if (item.promptMarker.line > row) continue;
      const end = item.endMarker && !item.endMarker.isDisposed ? item.endMarker.line : Number.POSITIVE_INFINITY;
      return row < end ? item : null;
    }
    return null;
  }

  /** 所有提示符所在行，包括正在输入的这一条，用于命令间跳转。 */
  promptLines() {
    const lines = this.liveCommands().map((item) => item.promptMarker.line);
    const current = this.current?.promptMarker;
    if (current && !current.isDisposed && current.line >= 0 && !lines.includes(current.line)) lines.push(current.line);
    return lines.sort((a, b) => a - b);
  }

  commandOutput(command: TerminalCommand) {
    const start = command.executedMarker;
    const end = command.endMarker;
    if (!start || !end || start.isDisposed || end.isDisposed) return "";
    const lines: string[] = [];
    const buffer = this.term.buffer.active;
    for (let row = start.line; row < end.line; row += 1) {
      const line = buffer.getLine(row);
      if (!line) continue;
      const text = line.translateToString(true);
      if (line.isWrapped && lines.length) lines[lines.length - 1] += text;
      else lines.push(text);
    }
    while (lines.length && !lines[lines.length - 1]) lines.pop();
    return lines.join("\n");
  }

  /** 命令及其输出占的行范围（含首尾），用来整块选中。 */
  commandRange(command: TerminalCommand) {
    const start = command.promptMarker.line;
    const endMarker = command.endMarker;
    const end = endMarker && !endMarker.isDisposed ? Math.max(start, endMarker.line - 1) : start;
    return { start, end };
  }

  private handle(sequence: ShellSequence) {
    if (!this.detected && sequence.kind !== "cwd") {
      this.detected = true;
      this.handlers.onDetected?.();
    }
    switch (sequence.kind) {
      case "promptStart":
        if (this.current?.executedMarker) this.finish(undefined);
        this.current = this.newCommand();
        this.state = "prompt";
        return;
      case "commandStart": {
        if (!this.current) this.current = this.newCommand();
        this.current.inputMarker = this.term.registerMarker(0);
        this.current.inputX = this.term.buffer.active.cursorX;
        this.state = "input";
        return;
      }
      case "executed": {
        this.reportsExecution = true;
        const current = this.current;
        if (!current) return;
        const cursorRow = this.cursorRow();
        const inputRow = current.inputMarker?.line ?? cursorRow;
        const lastRow = cursorRow > inputRow && this.term.buffer.active.cursorX === 0 ? cursorRow - 1 : cursorRow;
        current.command = this.readRows(inputRow, lastRow, current.inputX);
        current.executedMarker = this.term.registerMarker(0);
        current.startedAt = Date.now();
        current.cwd = this.cwd;
        this.state = "executing";
        return;
      }
      case "finished":
        this.finish(sequence.exitCode);
        return;
      case "cwd":
        this.setCwd(sequence.cwd);
    }
  }

  private finish(exitCode: number | undefined) {
    const current = this.current;
    if (!current) return;
    if (!current.executedMarker && !this.reportsExecution && this.state === "input" && current.inputMarker) {
      this.inferExecution(current);
    }
    if (!current.executedMarker) {
      this.current = null;
      this.state = "idle";
      return;
    }
    current.endMarker = this.term.registerMarker(0);
    current.exitCode = exitCode;
    current.finishedAt = Date.now();
    this.commands.push(current);
    if (this.commands.length > COMMAND_LIMIT) this.commands.splice(0, this.commands.length - COMMAND_LIMIT);
    this.current = null;
    this.state = "idle";
    this.handlers.onFinished?.(current);
  }

  /** cmd 等不报告执行时机的 shell：输入行（含折行）就是命令，下一行开始是输出。 */
  private inferExecution(current: TerminalCommand) {
    const buffer = this.term.buffer.active;
    const inputRow = current.inputMarker!.line;
    let lastRow = inputRow;
    while (buffer.getLine(lastRow + 1)?.isWrapped) lastRow += 1;
    const text = this.readRows(inputRow, lastRow, current.inputX);
    if (!text) return;
    current.command = text;
    current.cwd = this.cwd;
    current.executedMarker = this.term.registerMarker(lastRow + 1 - this.cursorRow());
    current.startedAt = Date.now();
  }

  private newCommand(): TerminalCommand {
    return {
      id: this.nextId++,
      promptMarker: this.term.registerMarker(0),
      inputX: 0,
      command: "",
      cwd: this.cwd,
    };
  }

  private setCwd(cwd: string) {
    if (cwd === this.cwd) return;
    this.cwd = cwd;
    this.handlers.onCwd?.(cwd);
  }

  private cursorRow() {
    const buffer = this.term.buffer.active;
    return buffer.baseY + buffer.cursorY;
  }

  private readRows(from: number, to: number, startX: number) {
    const buffer = this.term.buffer.active;
    const rows: InputRow[] = [];
    for (let row = from; row <= to; row += 1) {
      const line: IBufferLine | undefined = buffer.getLine(row);
      if (!line) continue;
      rows.push({ text: line.translateToString(true), wrapped: line.isWrapped });
    }
    return joinInputRows(rows, startX);
  }
}
