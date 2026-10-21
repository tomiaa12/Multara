//! Windows 控制台 Ctrl+C 时，避免 WebView2/Crashpad 把正常退出当成崩溃，
//! 弹出 “Error launching CrashSender.exe”。

#[link(name = "kernel32")]
extern "system" {
    fn SetErrorMode(mode: u32) -> u32;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn FreeConsole() -> i32;
}

const SEM_FAILCRITICALERRORS: u32 = 0x0001;
const SEM_NOGPFAULTERRORBOX: u32 = 0x0002;
const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;

unsafe extern "system" fn on_console_ctrl(_ctrl_type: u32) -> i32 {
    std::process::exit(0);
}

pub fn install() {
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
        SetConsoleCtrlHandler(Some(on_console_ctrl), 1);
        // 调试态由 `tauri dev` 共享控制台；先脱离，避免 Ctrl+C 打到 WebView2 子进程。
        #[cfg(debug_assertions)]
        {
            let _ = FreeConsole();
        }
    }
}
