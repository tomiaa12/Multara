//! 「在终端登录」用的命令行；以及检测外部是否还开着这个 CLI。

/// 外部（终端、IDE）是否还有这个 CLI 的进程。已打开的会话不会换号，还可能把原账号写回去。
pub fn process_running(command: &str) -> bool {
    if command.is_empty() || command.contains(['/', '\\']) {
        return false;
    }
    #[cfg(unix)]
    {
        std::process::Command::new("pgrep")
            .args(["-x", command])
            .output()
            .is_ok_and(|output| output.status.success())
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// 内嵌终端里输入的一行登录命令。Unix 是 POSIX shell，Windows 默认是 PowerShell（要 `&` 调用带引号的路径）。
pub fn command_line(program: &str, args: &[&str]) -> String {
    let parts = std::iter::once(program)
        .chain(args.iter().copied())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    if cfg!(windows) {
        format!("& {parts}")
    } else {
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_single_quotes() {
        assert_eq!(shell_quote("a'b"), r"'a'\''b'");
    }

    #[test]
    fn command_line_quotes_path_and_args() {
        let line = command_line("/Applications/My Tools/codex", &["login"]);
        assert!(line.ends_with("'/Applications/My Tools/codex' 'login'"));
    }

    #[test]
    fn paths_are_not_process_names() {
        assert!(!process_running("/usr/bin/codex"));
        assert!(!process_running(""));
    }
}
