//! Windows 任务栏图标：Tauri 2 只通过 `set_window_icon` 写 ICON_SMALL，
//! 不写 ICON_BIG；任务栏 / Alt+Tab 会回退到偏小或模糊的小图标。
//! 在无边框窗口就绪后，从 exe 资源再挂一次大小图标。

use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL, IMAGE_ICON, WM_SETICON,
};

/// EXE 里由 tauri-build 写入的默认图标资源（见 tauri-build set_icon_with_id "32512"）。
const APP_ICON_RESOURCE_ID: usize = 32512;
const ICON_SMALL_PX: i32 = 16;
const ICON_BIG_PX: i32 = 48;

pub fn apply(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe { set_icons(hwnd.0 as HWND) };
}

unsafe fn set_icons(hwnd: HWND) {
    let module = GetModuleHandleW(std::ptr::null());
    if module.is_null() {
        return;
    }
    let resource = APP_ICON_RESOURCE_ID as *const u16;
    let small = LoadImageW(module, resource, IMAGE_ICON, ICON_SMALL_PX, ICON_SMALL_PX, 0);
    let big = LoadImageW(module, resource, IMAGE_ICON, ICON_BIG_PX, ICON_BIG_PX, 0);
    if !small.is_null() {
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, small as LPARAM);
    }
    if !big.is_null() {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, big as LPARAM);
    }
}
