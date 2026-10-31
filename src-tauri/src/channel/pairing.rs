//! 配对尝试与未配对提示的限流，按（渠道连接，发送者）计数，只存在内存里。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// 连续猜错这么多次后锁定。
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(10 * 60);
/// 未配对 / 被锁定的发送者，这段时间内最多收到一条提示，避免被用来刷机器人 API 配额。
const NOTICE_INTERVAL: Duration = Duration::from_secs(10 * 60);

type SenderKey = (String, String);

#[derive(Default)]
struct Attempts {
    failures: u32,
    locked_until: Option<Instant>,
}

static ATTEMPTS: OnceLock<Mutex<HashMap<SenderKey, Attempts>>> = OnceLock::new();
static NOTICES: OnceLock<Mutex<HashMap<SenderKey, Instant>>> = OnceLock::new();

fn key(installation_id: &str, sender_id: &str) -> SenderKey {
    (installation_id.to_string(), sender_id.to_string())
}

fn attempts() -> std::sync::MutexGuard<'static, HashMap<SenderKey, Attempts>> {
    ATTEMPTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn is_locked(installation_id: &str, sender_id: &str) -> bool {
    let now = Instant::now();
    let mut map = attempts();
    map.retain(|_, item| item.locked_until.is_none_or(|until| until > now));
    map.get(&key(installation_id, sender_id))
        .is_some_and(|item| item.locked_until.is_some())
}

/// 记一次失败；这次失败触发锁定时返回 true。
pub fn record_failure(installation_id: &str, sender_id: &str) -> bool {
    let mut map = attempts();
    let item = map.entry(key(installation_id, sender_id)).or_default();
    item.failures += 1;
    if item.failures < MAX_FAILURES {
        return false;
    }
    item.failures = 0;
    item.locked_until = Some(Instant::now() + LOCKOUT);
    true
}

pub fn locked_notice() -> String {
    tr!(
        "配对码错误次数过多，请 {minutes} 分钟后再试。",
        minutes = LOCKOUT.as_secs() / 60
    )
}

pub fn clear(installation_id: &str, sender_id: &str) {
    attempts().remove(&key(installation_id, sender_id));
}

/// 这段时间内还没给这个发送者发过提示就返回 true 并记下。
pub fn allow_notice(installation_id: &str, sender_id: &str) -> bool {
    let now = Instant::now();
    let mut map = NOTICES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    map.retain(|_, at| now.duration_since(*at) < NOTICE_INTERVAL);
    let key = key(installation_id, sender_id);
    if map.contains_key(&key) {
        return false;
    }
    map.insert(key, now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_after_repeated_failures() {
        let (installation, sender) = ("test-installation", "test-sender-lock");
        for _ in 1..MAX_FAILURES {
            assert!(!record_failure(installation, sender));
        }
        assert!(!is_locked(installation, sender));
        assert!(record_failure(installation, sender));
        assert!(is_locked(installation, sender));
        clear(installation, sender);
        assert!(!is_locked(installation, sender));
    }

    #[test]
    fn throttles_notices_per_sender() {
        assert!(allow_notice("test-installation", "test-sender-notice"));
        assert!(!allow_notice("test-installation", "test-sender-notice"));
        assert!(allow_notice("test-installation", "test-sender-other"));
    }
}
