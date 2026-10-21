// @vitest-environment node
import { describe, expect, it } from "vitest";
import { detectPortInUse, detectQuickFixes } from "./terminal-quick-fix";

describe("detectQuickFixes", () => {
  it("offers to set the upstream after a rejected push", () => {
    const output = "fatal: The current branch dev has no upstream branch.\nTo push the current branch and set the remote as upstream, use\n\n    git push --set-upstream origin dev\n";
    expect(detectQuickFixes("git push", output, 128)).toEqual([
      { kind: "run", label: "运行 git push --set-upstream origin dev", command: "git push --set-upstream origin dev" },
    ]);
  });

  it("replaces a mistyped git subcommand", () => {
    const output = "git: 'stauts' is not a git command. See 'git --help'.\n\nThe most similar command is\n\tstatus\n";
    expect(detectQuickFixes("git stauts -s", output, 1)[0]).toMatchObject({ command: "git status -s" });
  });

  it("offers to free a busy port", () => {
    const output = "Error: listen EADDRINUSE: address already in use :::5173";
    expect(detectQuickFixes("pnpm dev", output, 1)).toContainEqual({ kind: "freePort", label: "结束占用端口 5173 的进程", port: 5173 });
  });

  it("links the pull request url printed by the remote", () => {
    const output = "remote: Create a pull request for 'dev' on GitHub by visiting:\nremote:      https://github.com/a/b/pull/new/dev\n";
    expect(detectQuickFixes("git push -u origin dev", output, 0)).toContainEqual({
      kind: "openUrl",
      label: "打开链接创建 Pull Request",
      url: "https://github.com/a/b/pull/new/dev",
    });
  });
});

describe("detectPortInUse", () => {
  it("reads common port-in-use messages", () => {
    expect(detectPortInUse("Port 3000 is already in use")).toBe(3000);
    expect(detectPortInUse("OSError: [Errno 98] Address already in use: ('0.0.0.0', 8000)")).toBe(8000);
    expect(detectPortInUse("nothing")).toBe(0);
  });
});
