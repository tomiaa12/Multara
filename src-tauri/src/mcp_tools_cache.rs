//! MCP 工具列表统一缓存：后台串行探测，前台只读缓存，避免进列表卡死。
//! 类似 Cursor：先缓存，再按需/后台刷新。

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::mcp_probe::McpProbe;
use crate::resources;

const EVENT_UPDATED: &str = "mcp-tools-cache-updated";
/// 未强制刷新时，成功缓存的复用窗口（与 Cursor「复用工具列表」接近）。
const FRESH_SECS: u64 = 30 * 60;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolsCacheEntry {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub probe: McpProbe,
    pub updated_at_ms: u64,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<String, McpToolsCacheEntry>,
    queue: VecDeque<String>,
    queued: HashSet<String>,
}

pub struct McpToolsCache {
    inner: Mutex<Inner>,
    path: PathBuf,
    running: AtomicBool,
}

impl McpToolsCache {
    pub fn load(path: PathBuf) -> Arc<Self> {
        let entries = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<McpToolsCacheEntry>>(&text).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect();
        Arc::new(Self {
            inner: Mutex::new(Inner {
                entries,
                queue: VecDeque::new(),
                queued: HashSet::new(),
            }),
            path,
            running: AtomicBool::new(false),
        })
    }

    pub fn list(&self) -> Vec<McpToolsCacheEntry> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        let mut rows: Vec<_> = guard.entries.values().cloned().collect();
        rows.sort_by(|left, right| left.id.cmp(&right.id));
        rows
    }

    pub fn get(&self, id: &str) -> Option<McpToolsCacheEntry> {
        self.inner.lock().ok()?.entries.get(id).cloned()
    }

    fn persist_locked(&self, guard: &Inner) {
        let rows: Vec<_> = guard.entries.values().cloned().collect();
        let Ok(text) = serde_json::to_string_pretty(&rows) else {
            return;
        };
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.path, text);
    }

    fn put(&self, entry: McpToolsCacheEntry) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.entries.insert(entry.id.clone(), entry);
            self.persist_locked(&guard);
        }
    }

    /// 后台排队刷新。`force` 时忽略新鲜缓存。立即返回。
    pub fn schedule(app: &AppHandle, resource_ids: Vec<String>, force: bool) {
        let Some(cache) = app.try_state::<Arc<McpToolsCache>>() else {
            return;
        };
        let now = now_ms();
        {
            let Ok(mut guard) = cache.inner.lock() else {
                return;
            };
            for id in resource_ids {
                let Some((owner, name)) = split_mcp_id(&id) else {
                    continue;
                };
                if !force {
                    if let Some(entry) = guard.entries.get(&id) {
                        let fresh = now.saturating_sub(entry.updated_at_ms) < FRESH_SECS * 1000;
                        if fresh && (entry.probe.ok || !entry.probe.tools.is_empty()) {
                            continue;
                        }
                    }
                }
                let _ = (owner, name);
                if guard.queued.insert(id.clone()) {
                    guard.queue.push_back(id);
                }
            }
            if guard.queue.is_empty() {
                return;
            }
        }
        if cache
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || worker_loop(app));
    }

    pub fn store_probe(app: &AppHandle, owner: &str, name: &str, probe: McpProbe) {
        let Some(cache) = app.try_state::<Arc<McpToolsCache>>() else {
            return;
        };
        let id = format!("mcp:{owner}:{name}");
        let entry = McpToolsCacheEntry {
            id: id.clone(),
            owner: owner.to_string(),
            name: name.to_string(),
            probe: probe.clone(),
            updated_at_ms: now_ms(),
        };
        cache.put(entry.clone());
        let _ = app.emit(EVENT_UPDATED, entry);
    }
}

fn worker_loop(app: AppHandle) {
    let Some(cache) = app.try_state::<Arc<McpToolsCache>>() else {
        return;
    };
    loop {
        let next = {
            let Ok(mut guard) = cache.inner.lock() else {
                break;
            };
            let id = guard.queue.pop_front();
            if let Some(ref id) = id {
                guard.queued.remove(id);
            }
            id
        };
        let Some(id) = next else {
            break;
        };
        let Some((owner, name)) = split_mcp_id(&id) else {
            continue;
        };
        if !resources::mcp_is_enabled(&owner, &name) {
            continue;
        }
        match resources::probe_mcp(&owner, &name) {
            Ok(probe) => McpToolsCache::store_probe(&app, &owner, &name, probe),
            Err(error) => {
                let mut probe = McpProbe::default();
                probe.status = "error".into();
                probe.error = error;
                McpToolsCache::store_probe(&app, &owner, &name, probe);
            }
        }
    }
    cache.running.store(false, Ordering::SeqCst);
    // 排队期间又有新任务
    let has_more = cache
        .inner
        .lock()
        .map(|guard| !guard.queue.is_empty())
        .unwrap_or(false);
    if has_more
        && cache
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || worker_loop(app));
    }
}

fn split_mcp_id(id: &str) -> Option<(String, String)> {
    let mut parts = id.splitn(3, ':');
    if parts.next() != Some("mcp") {
        return None;
    }
    let owner = parts.next()?.to_string();
    let name = parts.next()?.to_string();
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some((owner, name))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}
