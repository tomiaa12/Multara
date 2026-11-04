use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{
    AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::commands::setting_value;
use crate::database::AppState;

/// Help → 切换开发者工具（对齐 VS Code `workbench.action.toggleDevTools` / ⌘⌥I）。
pub const TOGGLE_DEVTOOLS_ID: &str = "toggle-devtools";
/// 查看 → 自动换行（对齐 VS Code `editor.action.toggleWordWrap` / ⌥Z）。
pub const TOGGLE_WORD_WRAP_ID: &str = "toggle-word-wrap";

static WORD_WRAP: AtomicBool = AtomicBool::new(false);

fn word_wrap_from_db<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let Ok(db) = state.lock_db() else {
        return false;
    };
    setting_value(&db, "wordWrap", "false") == "true"
}

fn set_word_wrap_checked<R: Runtime>(app: &AppHandle<R>, checked: bool) {
    WORD_WRAP.store(checked, Ordering::Relaxed);
    let Some(menu) = app.menu() else {
        return;
    };
    for item in menu.items().unwrap_or_default() {
        let Some(sub) = item.as_submenu() else {
            continue;
        };
        let Some(kind) = sub.get(TOGGLE_WORD_WRAP_ID) else {
            continue;
        };
        if let Some(check) = kind.as_check_menuitem() {
            let _ = check.set_checked(checked);
        }
        break;
    }
}

/// 设置页等非菜单路径改了 wordWrap 时，同步菜单勾选。
pub fn sync_word_wrap<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    set_word_wrap_checked(app, enabled);
}

pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        TOGGLE_DEVTOOLS_ID => {
            let Some(window) = app.get_webview_window("main") else {
                return;
            };
            if window.is_devtools_open() {
                window.close_devtools();
            } else {
                window.open_devtools();
            }
        }
        TOGGLE_WORD_WRAP_ID => {
            // CheckMenuItem 点击时系统已翻转勾选；以我们的状态为准再写回，避免和设置页打架。
            let next = !WORD_WRAP.load(Ordering::Relaxed);
            set_word_wrap_checked(app, next);
            let _ = app.emit("menu-word-wrap", next);
        }
        _ => {}
    }
}

pub fn app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let package = app.package_info();
    let config = app.config();
    let name = package.name.clone();
    let about = tr!("关于 {name}", name = name);
    let hide = tr!("隐藏 {name}", name = name);
    let quit = tr!("退出 {name}", name = name);
    let about_metadata = AboutMetadata {
        name: Some(name.clone()),
        version: Some(package.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config
            .bundle
            .publisher
            .clone()
            .map(|publisher| vec![publisher]),
        ..Default::default()
    };

    let app_menu = Submenu::with_items(
        app,
        &name,
        true,
        &[
            &PredefinedMenuItem::about(app, Some(&about), Some(about_metadata))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, Some(&tr!("服务")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(&hide))?,
            &PredefinedMenuItem::hide_others(app, Some(&tr!("隐藏其他")))?,
            &PredefinedMenuItem::show_all(app, Some(&tr!("全部显示")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(&quit))?,
        ],
    )?;

    let file_menu = Submenu::with_items(
        app,
        &tr!("文件"),
        true,
        &[&PredefinedMenuItem::close_window(app, Some(&tr!("关闭")))?],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        &tr!("编辑"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&tr!("撤销")))?,
            &PredefinedMenuItem::redo(app, Some(&tr!("重做")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some(&tr!("剪切")))?,
            &PredefinedMenuItem::copy(app, Some(&tr!("拷贝")))?,
            &PredefinedMenuItem::paste(app, Some(&tr!("粘贴")))?,
            &PredefinedMenuItem::select_all(app, Some(&tr!("全选")))?,
        ],
    )?;

    let word_wrap = word_wrap_from_db(app);
    WORD_WRAP.store(word_wrap, Ordering::Relaxed);
    let toggle_word_wrap = CheckMenuItem::with_id(
        app,
        TOGGLE_WORD_WRAP_ID,
        &tr!("自动换行"),
        true,
        word_wrap,
        Some("Alt+Z"),
    )?;
    let view_menu = Submenu::with_items(
        app,
        &tr!("查看"),
        true,
        &[
            &toggle_word_wrap,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, Some(&tr!("进入全屏幕")))?,
        ],
    )?;

    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        &tr!("窗口"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(&tr!("最小化")))?,
            &PredefinedMenuItem::maximize(app, Some(&tr!("缩放")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::bring_all_to_front(app, Some(&tr!("前置全部窗口")))?,
        ],
    )?;

    let toggle_devtools = MenuItem::with_id(
        app,
        TOGGLE_DEVTOOLS_ID,
        &tr!("切换开发者工具"),
        true,
        Some("CmdOrCtrl+Alt+I"),
    )?;
    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        &tr!("帮助"),
        true,
        &[&toggle_devtools],
    )?;

    Menu::with_items(
        app,
        &[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )
}
