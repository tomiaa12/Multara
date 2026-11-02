//! Multara 自己的账号库：`<app_data>/runtime-accounts/<runtime>.json`，0600，不进导出和同步。

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use super::item::write_private;

const VERSION: u32 = 1;

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default)]
    pub accounts: Vec<Account>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub identity_key: String,
    /// 从凭据里认出的显示名
    #[serde(default)]
    pub label: Option<String>,
    /// 用户改的名字，优先于 `label`
    #[serde(default)]
    pub name: Option<String>,
    pub captured_at: i64,
    pub items: Vec<StoredItem>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredItem {
    pub tag: String,
    /// base64；`None` 表示这一项在该账号下不存在
    #[serde(default)]
    pub value: Option<String>,
}

impl Account {
    /// 按凭据项的 tag 取值，和登记顺序对齐；库里没有的项按不存在处理。
    pub fn values(&self, tags: &[String]) -> Vec<Option<Vec<u8>>> {
        tags.iter()
            .map(|tag| {
                self.items
                    .iter()
                    .find(|item| &item.tag == tag)
                    .and_then(|item| item.value.as_deref())
                    .and_then(|value| STANDARD.decode(value).ok())
            })
            .collect()
    }
}

pub fn encode(tags: &[String], values: &[Option<Vec<u8>>]) -> Vec<StoredItem> {
    tags.iter()
        .zip(values)
        .map(|(tag, value)| StoredItem {
            tag: tag.clone(),
            value: value.as_deref().map(|bytes| STANDARD.encode(bytes)),
        })
        .collect()
}

pub fn path(dir: &Path, runtime: &str) -> PathBuf {
    dir.join(format!("{runtime}.json"))
}

pub fn load(path: &Path) -> Result<Store, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| tr!("账号库文件已损坏：{path}", path = path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Store::default()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn save(path: &Path, store: &mut Store) -> Result<(), String> {
    store.version = VERSION;
    let bytes = serde_json::to_vec_pretty(store).map_err(|error| error.to_string())?;
    write_private(path, &bytes)
}
