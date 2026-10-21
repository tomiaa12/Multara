use std::{ffi::OsStr, process::Command};

/// 后台子进程：Windows 上不弹出 cmd / 控制台窗口。
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
