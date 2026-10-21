export type TerminalQuickFix =
  | { kind: "run"; label: string; command: string }
  | { kind: "freePort"; label: string; port: number }
  | { kind: "openUrl"; label: string; url: string };

const MAX_FIXES = 5;

/** 根据刚结束的命令与输出给出修复建议，规则与 VS Code 内置的终端快速修复对应。 */
export function detectQuickFixes(command: string, output: string, exitCode: number | undefined): TerminalQuickFix[] {
  const fixes: TerminalQuickFix[] = [];
  const push = (fix: TerminalQuickFix) => {
    if (fixes.some((item) => item.label === fix.label)) return;
    fixes.push(fix);
  };
  const trimmed = command.trim();

  const upstream = output.match(/git push --set-upstream (\S+) (\S+)/);
  if (upstream) {
    push({ kind: "run", label: `运行 git push --set-upstream ${upstream[1]} ${upstream[2]}`, command: `git push --set-upstream ${upstream[1]} ${upstream[2]}` });
  }

  const similar = output.match(/git: '([^']+)' is not a git command[\s\S]*?The most similar commands? (?:is|are)\s*\n((?:\s+\S+\s*\n?)+)/);
  if (similar && /^git\s/.test(trimmed)) {
    const typo = similar[1]!;
    for (const suggestion of similar[2]!.split("\n").map((item) => item.trim()).filter(Boolean)) {
      const fixed = trimmed.replace(new RegExp(`^git\\s+${escapeRegExp(typo)}\\b`), `git ${suggestion}`);
      if (fixed !== trimmed) push({ kind: "run", label: `运行 ${fixed}`, command: fixed });
    }
  }

  const pathspec = output.match(/error: pathspec '([^']+)' did not match any file\(s\) known to git/);
  if (pathspec && /^git\s+(checkout|switch)\b/.test(trimmed)) {
    const verb = /^git\s+switch\b/.test(trimmed) ? "switch -c" : "checkout -b";
    push({ kind: "run", label: `创建分支：git ${verb} ${pathspec[1]}`, command: `git ${verb} ${pathspec[1]}` });
  }

  const pullRequest = output.match(/(?:Create a pull request|create a merge request)[^\n]*\n(?:remote:)?\s*(https?:\/\/\S+)/i);
  if (pullRequest) push({ kind: "openUrl", label: "打开链接创建 Pull Request", url: pullRequest[1]! });

  if (exitCode !== 0) {
    const port = detectPortInUse(output);
    if (port) push({ kind: "freePort", label: `结束占用端口 ${port} 的进程`, port });
  }

  const pip = output.match(/To update, run:\s*(\S*python[\w.]*\s+-m\s+pip\s+install\s+--upgrade\s+pip)/i);
  if (pip) push({ kind: "run", label: `运行 ${pip[1]}`, command: pip[1]! });

  return fixes.slice(0, MAX_FIXES);
}

export function detectPortInUse(output: string) {
  const patterns = [
    /EADDRINUSE[^\n]*?:(\d{2,5})\b/,
    /address already in use[^\n]*?:(\d{2,5})\b/i,
    /port (\d{2,5}) is (?:already )?in use/i,
    /端口\s*(\d{2,5})\s*(?:已被|被)占用/,
    /Only one usage of each socket address[^\n]*?(\d{2,5})/i,
    /Address already in use[^\n]*\((\d{2,5})\)/i,
    /\('[\d.]+',\s*(\d{2,5})\)/,
  ];
  for (const pattern of patterns) {
    const match = output.match(pattern);
    const port = match ? Number(match[1]) : 0;
    if (port > 0 && port < 65536) return port;
  }
  return 0;
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
