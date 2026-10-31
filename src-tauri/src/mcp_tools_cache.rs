//! MCP 工具列表统一缓存：后台串行探测，前台只读缓存，避免进列表卡死。
//! 类似 Cursor：先缓存，再按需/后台刷新。

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    panic::{self, AssertUnwindSafe},
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
/// 失败或需要授权的探测，未强制刷新时这段时间内不重新排队，避免反复起 `npx`。
const FAILURE_BACKOFF_SECS: u64 = 10 * 60;

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
    persist: Mutex<()>,
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
            persist: Mutex::new(()),
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

    /// 在 `inner` 锁里只做插入和克隆快照，序列化与写盘放到锁外，`list()` / `get()` 不被 IO 挡住；
    /// `persist` 锁保证并发 `put` 按顺序落盘，旧快照不会覆盖新快照。
    fn put(&self, entry: McpToolsCacheEntry) {
        let Ok(_persist) = self.persist.lock() else {
            return;
        };
        let rows: Vec<_> = {
            let Ok(mut guard) = self.inner.lock() else {
                return;
            };
            guard.entries.insert(entry.id.clone(), entry);
            guard.entries.values().cloned().collect()
        };
        if let Ok(text) = serde_json::to_string_pretty(&rows) {
            let _ = crate::mcp_vars::write_private_atomic(&self.path, text.as_bytes());
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
                        let succeeded = entry.probe.ok || !entry.probe.tools.is_empty();
                        let window = if succeeded {
                            FRESH_SECS
                        } else {
                            FAILURE_BACKOFF_SECS
                        };
                        if now.saturating_sub(entry.updated_at_ms) < window * 1000 {
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
        // 探测 panic 不能让 `running` 永远停在 true，否则缓存再也不会探测
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            resources::mcp_is_enabled(&owner, &name).then(|| resources::probe_mcp(&owner, &name))
        }))
        .unwrap_or_else(|_| Some(Err(tr!("探测时发生内部错误"))));
        match outcome {
            None => {}
            Some(Ok(probe)) => McpToolsCache::store_probe(&app, &owner, &name, probe),
            Some(Err(error)) => {
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
