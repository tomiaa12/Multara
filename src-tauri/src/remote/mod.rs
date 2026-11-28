//! 多设备远程访问：一台主机，多台控制端。设计见 docs/多设备-技术方案.md。

mod client;
pub mod commands;
mod events;
mod frame;
mod gateway;
mod identity;
mod ipc;
mod network;
mod pairing;
pub mod permission;
mod rules;
pub mod scope;
mod send;
mod server;
pub mod store;
mod terminals;
mod tls;

use tauri::AppHandle;

/// 应用启动时调用：按设置开启主机监听，并挂上事件转发。
pub fn start(app: &AppHandle) {
    events::install(app);
    server::start(app);
}
