//! 系统钥匙串里的一条通用密码。
//!
//! macOS 上经 `/usr/bin/security` 读写：agy、Claude Code 这些 CLI 自己也是调它存登录，
//! 条目的访问控制信任这个程序，所以读写不会弹授权框。其他平台暂不支持。

/// 测试换成内存实现，默认测试不碰真实钥匙串。
pub trait Keychain: Send + Sync {
    /// 条目不存在时为 `None`。
    fn read(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    /// 整条覆盖写入，不存在时新建。
    fn write(&self, service: &str, account: &str, value: &str) -> Result<(), String>;
}

pub struct SystemKeychain;

pub fn supported() -> bool {
    cfg!(target_os = "macos")
}

#[cfg(target_os = "macos")]
const SECURITY: &str = "/usr/bin/security";
/// `security` 找不到条目时的退出码。
#[cfg(target_os = "macos")]
const ITEM_NOT_FOUND: i32 = 44;

#[cfg(target_os = "macos")]
impl Keychain for SystemKeychain {
    fn read(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        let output = std::process::Command::new(SECURITY)
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.code() == Some(ITEM_NOT_FOUND) {
            return Ok(None);
        }
        if !output.status.success() {
            return Err(tr!(
                "读取钥匙串失败：{error}",
                error = String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let value = String::from_utf8_lossy(&output.stdout);
        Ok(Some(value.trim_end_matches(['\r', '\n']).to_string()))
    }

    fn write(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        let output = std::process::Command::new(SECURITY)
            .args(["add-generic-password", "-U", "-s", service, "-a", account, "-w", value])
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        Err(tr!(
            "写入钥匙串失败：{error}",
            error = String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(not(target_os = "macos"))]
impl Keychain for SystemKeychain {
    fn read(&self, _service: &str, _account: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn write(&self, _service: &str, _account: &str, _value: &str) -> Result<(), String> {
        Err(tr!("当前系统不支持钥匙串账号"))
    }
}

#[cfg(test)]
pub mod memory {
    use std::{collections::HashMap, sync::Mutex};

    use super::Keychain;

    #[derive(Default)]
    pub struct MemoryKeychain {
        pub items: Mutex<HashMap<(String, String), String>>,
    }

    impl Keychain for MemoryKeychain {
        fn read(&self, service: &str, account: &str) -> Result<Option<String>, String> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .get(&(service.to_string(), account.to_string()))
                .cloned())
        }

        fn write(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
            self.items
                .lock()
                .unwrap()
                .insert((service.to_string(), account.to_string()), value.to_string());
            Ok(())
        }
    }
}
