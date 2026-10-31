//! Cursor 全局 MCP 启停不在 mcp.json，而在 state.vscdb 的
//! `cursor/disabledGlobalMcpServers`（值为 `user-<name>` 等）。

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, TransactionBehavior};

const DISABLED_KEY: &str = "cursor/disabledGlobalMcpServers";
/// Cursor 正在用这个库，等锁太久会卡住调用方（可能在主线程），超时就放弃。
const STATE_DB_BUSY_TIMEOUT: Duration = Duration::from_millis(500);

/// Cursor 用户级 mcp.json 里的服务器，在 state 里对应 `user-<name>`。
pub fn user_server_id(name: &str) -> String {
    format!("user-{name}")
}

pub fn state_db_path(home: &Path) -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(|path| PathBuf::from(path).join("Cursor/User/globalStorage/state.vscdb"))
            .unwrap_or_else(|| home.join("AppData/Roaming/Cursor/User/globalStorage/state.vscdb"))
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
    } else {
        home.join(".config/Cursor/User/globalStorage/state.vscdb")
    }
}

pub fn disabled_global_ids(home: &Path) -> HashSet<String> {
    disabled_global_ids_at(&state_db_path(home))
}

pub fn is_user_server_disabled(home: &Path, name: &str) -> bool {
    disabled_global_ids(home).contains(&user_server_id(name))
}

pub fn set_user_server_enabled(home: &Path, name: &str, enabled: bool) -> Result<(), String> {
    set_user_server_enabled_at(&state_db_path(home), name, enabled)
}

fn disabled_global_ids_at(path: &Path) -> HashSet<String> {
    let Ok(conn) = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return HashSet::new();
    };
    if conn.busy_timeout(STATE_DB_BUSY_TIMEOUT).is_err() {
        return HashSet::new();
    }
    read_disabled_list(&conn)
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn set_user_server_enabled_at(path: &Path, name: &str, enabled: bool) -> Result<(), String> {
    if !path.is_file() {
        return Err(tr!("找不到 Cursor 状态库（state.vscdb）"));
    }
    let mut conn = Connection::open(path)
        .map_err(|error| tr!("打开 Cursor 状态库失败：{error}", error = error))?;
    conn.busy_timeout(STATE_DB_BUSY_TIMEOUT)
        .map_err(|error| error.to_string())?;
    // 读和写放在同一个写事务里，中间不会插进 Cursor 的写入
    let conn = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| tr!("Cursor 状态库正忙，请稍后再试：{error}", error = error))?;
    let id = user_server_id(name);
    let mut list = read_disabled_list(&conn).unwrap_or_default();
    let changed = if enabled {
        let before = list.len();
        list.retain(|item| item != &id);
        list.len() != before
    } else if list.iter().any(|item| item == &id) {
        false
    } else {
        list.push(id);
        true
    };
    if !changed {
        return Ok(());
    }
    list.sort();
    let value = serde_json::to_string(&list).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO ItemTable(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![DISABLED_KEY, value],
    )
    .map_err(|error| tr!("写入 Cursor MCP 启停失败：{error}", error = error))?;
    conn.commit()
        .map_err(|error| tr!("写入 Cursor MCP 启停失败：{error}", error = error))
}

fn read_disabled_list(conn: &Connection) -> Result<Vec<String>, String> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?1",
            [DISABLED_KEY],
            |row| {
                let text = match row.get_ref(0)? {
                    ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                        String::from_utf8_lossy(bytes).into_owned()
                    }
                    _ => String::new(),
                };
                Ok(text)
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(text) = value.filter(|item| !item.is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn temp_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);",
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn user_server_id_prefixes_name() {
        assert_eq!(user_server_id("chrome-devtools"), "user-chrome-devtools");
    }

    #[test]
    fn toggles_disabled_list_in_state_db() {
        let (_dir, path) = temp_db();
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO ItemTable(key, value) VALUES(?1, ?2)",
            params![DISABLED_KEY, r#"["user-other"]"#],
        )
        .unwrap();
        drop(conn);

        set_user_server_enabled_at(&path, "chrome-devtools", false).unwrap();
        let disabled = disabled_global_ids_at(&path);
        assert!(disabled.contains("user-chrome-devtools"));
        assert!(disabled.contains("user-other"));

        set_user_server_enabled_at(&path, "chrome-devtools", true).unwrap();
        let disabled = disabled_global_ids_at(&path);
        assert!(!disabled.contains("user-chrome-devtools"));
        assert!(disabled.contains("user-other"));
    }
}
