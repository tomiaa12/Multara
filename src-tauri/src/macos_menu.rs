use tauri::menu::{
    AboutMetadata, Menu, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Runtime};

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

    let view_menu = Submenu::with_items(
        app,
        &tr!("显示"),
        true,
        &[&PredefinedMenuItem::fullscreen(app, Some(&tr!("进入全屏幕")))?],
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

    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        &tr!("帮助"),
        true,
        &[] as &[&dyn tauri::menu::IsMenuItem<R>],
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
