//! 探测各 CLI 订阅用量（对齐 t3code Usage limits）。
//!
//! 用本机已登录凭据打官方用量接口，不经过 Multara 任务统计。

pub mod commands;
mod creds;
mod probes;

use serde::Serialize;

const SESSION_MINS: u64 = 5 * 60;
const WEEK_MINS: u64 = 7 * 24 * 60;
const MONTH_MINS: u64 = 30 * 24 * 60;
const PROBE_TIMEOUT_SECS: u32 = 15;

const SUPPORTED: &[&str] = &[
    "codex",
    "claude",
    "grok",
    "cursor",
    "opencode",
    "gemini",
    "antigravity",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUsageWindow {
    pub id: String,
    /// session | weekly | monthly | other
    pub kind: String,
    pub label: String,
    pub used_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_duration_mins: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUsageUnavailable {
    /// unsupported | probeFailed | noCredentials
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUsage {
    pub runtime_id: String,
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub checked_at: String,
    pub windows: Vec<RuntimeUsageWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<RuntimeUsageUnavailable>,
}

pub fn is_supported(runtime_id: &str) -> bool {
    SUPPORTED.contains(&runtime_id)
}

pub fn supported_ids() -> &'static [&'static str] {
    SUPPORTED
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn clamp_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 100.0)
}

fn unavailable(runtime_id: &str, reason: &str, message: Option<String>) -> RuntimeUsage {
    RuntimeUsage {
        runtime_id: runtime_id.to_string(),
        supported: is_supported(runtime_id),
        email: None,
        plan: None,
        checked_at: now_iso(),
        windows: Vec::new(),
        unavailable: Some(RuntimeUsageUnavailable {
            reason: reason.to_string(),
            message,
        }),
    }
}

fn with_windows(
    runtime_id: &str,
    email: Option<String>,
    plan: Option<String>,
    windows: Vec<RuntimeUsageWindow>,
) -> RuntimeUsage {
    if windows.is_empty() {
        return unavailable(
            runtime_id,
            "unsupported",
            Some(tr!("这个账号没有可用的用量窗口")),
        );
    }
    RuntimeUsage {
        runtime_id: runtime_id.to_string(),
        supported: true,
        email,
        plan,
        checked_at: now_iso(),
        windows,
        unavailable: None,
    }
}

/// 同步探测单个运行时用量。
pub fn probe(runtime_id: &str) -> RuntimeUsage {
    if !is_supported(runtime_id) {
        return unavailable(runtime_id, "unsupported", None);
    }
    let home = match creds::home_dir() {
        Ok(path) => path,
        Err(message) => return unavailable(runtime_id, "probeFailed", Some(message)),
    };
    let result = match runtime_id {
        "codex" => probes::codex(&home),
        "claude" => probes::claude(&home),
        "grok" => probes::grok(&home),
        "cursor" => probes::cursor(&home),
        "opencode" => probes::opencode(&home),
        "gemini" => probes::gemini(&home),
        "antigravity" => probes::antigravity(&home),
        _ => Err(ProbeError::Unsupported),
    };
    match result {
        Ok(ok) => with_windows(runtime_id, ok.email, ok.plan, ok.windows),
        Err(ProbeError::NoCredentials) => unavailable(
            runtime_id,
            "noCredentials",
            Some(tr!("未找到可用登录，请先在终端登录该 CLI")),
        ),
        Err(ProbeError::Unsupported) => unavailable(runtime_id, "unsupported", None),
        Err(ProbeError::Failed(message)) => unavailable(runtime_id, "probeFailed", Some(message)),
    }
}

struct ProbeOk {
    email: Option<String>,
    plan: Option<String>,
    windows: Vec<RuntimeUsageWindow>,
}

enum ProbeError {
    NoCredentials,
    Unsupported,
    Failed(String),
}

fn kind_for_duration(mins: u64) -> &'static str {
    if mins >= MONTH_MINS {
        "monthly"
    } else if mins >= WEEK_MINS {
        "weekly"
    } else {
        "session"
    }
}

fn label_for_kind(kind: &str) -> String {
    match kind {
        "session" => tr!("会话").to_string(),
        "weekly" => tr!("每周").to_string(),
        "monthly" => tr!("每月").to_string(),
        _ => tr!("订阅").to_string(),
    }
}

fn iso_from_epoch_secs(secs: f64) -> Option<String> {
    if !secs.is_finite() || secs <= 0.0 {
        return None;
    }
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

fn iso_from_epoch_ms(ms: f64) -> Option<String> {
    if !ms.is_finite() || ms <= 0.0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_list_matches_t3_local_probes() {
        assert!(is_supported("codex"));
        assert!(is_supported("claude"));
        assert!(is_supported("grok"));
        assert!(is_supported("cursor"));
        assert!(is_supported("opencode"));
        assert!(is_supported("gemini"));
        assert!(is_supported("antigravity"));
        assert!(!is_supported("qwen"));
    }

    #[test]
    fn clamp_and_kind() {
        assert_eq!(clamp_percent(150.0), 100.0);
        assert_eq!(kind_for_duration(SESSION_MINS), "session");
        assert_eq!(kind_for_duration(WEEK_MINS), "weekly");
        assert_eq!(kind_for_duration(MONTH_MINS), "monthly");
    }

    #[test]
    fn probe_unknown_is_unsupported() {
        let usage = probe("qwen");
        assert!(!usage.supported);
        assert_eq!(
            usage.unavailable.as_ref().map(|item| item.reason.as_str()),
            Some("unsupported")
        );
    }

    #[test]
    fn parse_cloudcode_summary_bucket() {
        let summary = serde_json::json!({
            "groups": [{
                "displayName": "Gemini",
                "buckets": [{
                    "bucketId": "gemini-5h",
                    "window": "5h",
                    "remainingFraction": 0.75,
                    "resetTime": "2026-11-16T00:00:00Z"
                }]
            }]
        });
        let windows = probes::parse_cloudcode_summary_for_test(&summary);
        assert_eq!(windows.len(), 1);
        assert!((windows[0].used_percent - 25.0).abs() < 0.01);
        assert_eq!(windows[0].kind, "session");
    }

    /// 本机有 Codex 登录时验证 wham/usage；没有网络或凭据时跳过。
    #[test]
    fn live_codex_probe_when_logged_in() {
        if creds::codex_auth(&creds::home_dir().unwrap_or_default()).is_none() {
            return;
        }
        let usage = probe("codex");
        assert!(usage.supported);
        if let Some(unavailable) = &usage.unavailable {
            // 网络/鉴权失败时保留结果但不强求 windows
            eprintln!("codex unavailable: {unavailable:?}");
            return;
        }
        assert!(!usage.windows.is_empty(), "{usage:?}");
        eprintln!(
            "codex plan={:?} windows={:?}",
            usage.plan,
            usage
                .windows
                .iter()
                .map(|window| (&window.label, window.used_percent))
                .collect::<Vec<_>>()
        );
    }
}
