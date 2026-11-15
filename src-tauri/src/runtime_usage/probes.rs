//! 各 CLI 官方用量接口探测。

use serde_json::Value;

use super::{
    clamp_percent, creds, iso_from_epoch_ms, iso_from_epoch_secs, kind_for_duration, label_for_kind,
    ProbeError, ProbeOk, RuntimeUsageWindow, MONTH_MINS, PROBE_TIMEOUT_SECS, SESSION_MINS,
    WEEK_MINS,
};
use std::path::Path;

const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const GROK_USAGE_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const CURSOR_USAGE_URL: &str =
    "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
const OPENCODE_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
const CLOUDCODE_DAILY: &str = "https://daily-cloudcode-pa.googleapis.com";
const CLOUDCODE_PROD: &str = "https://cloudcode-pa.googleapis.com";
const GROK_CLI_VERSION: &str = "1.0.77";
const CLAUDE_USER_AGENT: &str = "claude-code/2.1.273";
const GEMINI_USER_AGENT: &str = "gemini-cli/0.0.0";
const ANTIGRAVITY_USER_AGENT: &str =
    "antigravity/hub/2.24.0 (aidev_client; os_type=windows; arch=x64; cl=963137146)";

fn get_json(url: &str, headers: &[(String, String)]) -> Result<Value, ProbeError> {
    let response = crate::http_client::request("GET", url, headers, None, PROBE_TIMEOUT_SECS)
        .map_err(ProbeError::Failed)?;
    if response.status == 401 || response.status == 403 {
        return Err(ProbeError::Failed(tr!(
            "用量接口拒绝访问（HTTP {status}），请重新登录该 CLI",
            status = response.status
        )));
    }
    if !(200..300).contains(&response.status) {
        return Err(ProbeError::Failed(tr!(
            "用量接口失败（HTTP {status}）",
            status = response.status
        )));
    }
    serde_json::from_str(&response.body).map_err(|error| ProbeError::Failed(error.to_string()))
}

fn post_json(url: &str, headers: &[(String, String)], body: &str) -> Result<Value, ProbeError> {
    let response = crate::http_client::request("POST", url, headers, Some(body), PROBE_TIMEOUT_SECS)
        .map_err(ProbeError::Failed)?;
    if response.status == 401 || response.status == 403 {
        return Err(ProbeError::Failed(tr!(
            "用量接口拒绝访问（HTTP {status}），请重新登录该 CLI",
            status = response.status
        )));
    }
    if !(200..300).contains(&response.status) {
        return Err(ProbeError::Failed(tr!(
            "用量接口失败（HTTP {status}）",
            status = response.status
        )));
    }
    serde_json::from_str(&response.body).map_err(|error| ProbeError::Failed(error.to_string()))
}

pub fn codex(home: &Path) -> Result<ProbeOk, ProbeError> {
    let auth = creds::codex_auth(home).ok_or(ProbeError::NoCredentials)?;
    let mut headers = vec![
        (
            "Authorization".into(),
            format!("Bearer {}", auth.access_token),
        ),
        ("Content-Type".into(), "application/json".into()),
        ("OpenAI-Beta".into(), "codex-1".into()),
        ("Originator".into(), "Codex Desktop".into()),
    ];
    if let Some(account_id) = &auth.account_id {
        headers.push(("Chatgpt-Account-Id".into(), account_id.clone()));
    }
    let usage = get_json(CODEX_USAGE_URL, &headers)?;
    let plan = usage
        .get("plan_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or(auth.plan);
    let is_monthly = matches!(plan.as_deref(), Some("free") | Some("go"));
    let rate = usage.get("rate_limit");
    let mut windows = Vec::new();
    for (id, key, fallback_mins) in [
        ("primary", "primary_window", if is_monthly { MONTH_MINS } else { SESSION_MINS }),
        ("secondary", "secondary_window", WEEK_MINS),
    ] {
        let Some(window) = rate.and_then(|value| value.get(key)).filter(|value| !value.is_null())
        else {
            continue;
        };
        let Some(used) = window.get("used_percent").and_then(Value::as_f64) else {
            continue;
        };
        let duration = window
            .get("limit_window_seconds")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|secs| (secs / 60.0).round() as u64)
            .unwrap_or(fallback_mins);
        let kind = kind_for_duration(duration);
        let resets_at = window
            .get("reset_at")
            .and_then(Value::as_f64)
            .and_then(iso_from_epoch_secs);
        windows.push(RuntimeUsageWindow {
            id: id.to_string(),
            kind: kind.to_string(),
            label: label_for_kind(kind),
            used_percent: clamp_percent(used),
            resets_at,
            window_duration_mins: Some(duration),
        });
    }
    Ok(ProbeOk {
        email: auth.email,
        plan: plan.map(codex_plan_label),
        windows,
    })
}

fn codex_plan_label(plan: String) -> String {
    match plan.to_ascii_lowercase().as_str() {
        "free" => "Free".into(),
        "go" => "Go".into(),
        "plus" => "Plus".into(),
        "pro" => "Pro".into(),
        "team" => "Team".into(),
        "enterprise" => "Enterprise".into(),
        other => other.to_string(),
    }
}

pub fn claude(home: &Path) -> Result<ProbeOk, ProbeError> {
    let auth = creds::claude_auth(home).ok_or(ProbeError::NoCredentials)?;
    let headers = vec![
        (
            "Authorization".into(),
            format!("Bearer {}", auth.access_token),
        ),
        ("anthropic-beta".into(), "oauth-2025-04-20".into()),
        ("User-Agent".into(), CLAUDE_USER_AGENT.into()),
    ];
    let usage = get_json(CLAUDE_USAGE_URL, &headers)?;
    let mut windows = Vec::new();
    push_claude_window(&mut windows, "five_hour", "session", tr!("会话"), SESSION_MINS, usage.get("five_hour"));
    push_claude_window(&mut windows, "seven_day", "weekly", tr!("每周"), WEEK_MINS, usage.get("seven_day"));
    if let Some(limits) = usage.get("limits").and_then(Value::as_array) {
        for limit in limits {
            if limit.get("kind").and_then(Value::as_str) != Some("weekly_scoped") {
                continue;
            }
            let Some(percent) = limit.get("percent").and_then(Value::as_f64) else {
                continue;
            };
            let display = limit
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Model");
            let id = format!(
                "seven_day_{}",
                display
                    .to_ascii_lowercase()
                    .chars()
                    .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
                    .collect::<String>()
            );
            windows.push(RuntimeUsageWindow {
                id,
                kind: "weekly".into(),
                label: tr!("每周 · {name}", name = display),
                used_percent: clamp_percent(percent),
                resets_at: limit
                    .get("resets_at")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                window_duration_mins: Some(WEEK_MINS),
            });
        }
    }
    if let Some(extra) = usage.get("extra_usage").filter(|value| !value.is_null()) {
        if extra.get("is_enabled").and_then(Value::as_bool) == Some(true) {
            if let Some(percent) = extra.get("utilization").and_then(Value::as_f64) {
                windows.push(RuntimeUsageWindow {
                    id: "extra_usage".into(),
                    kind: "monthly".into(),
                    label: tr!("额外用量").to_string(),
                    used_percent: clamp_percent(percent),
                    resets_at: None,
                    window_duration_mins: Some(MONTH_MINS),
                });
            }
        }
    }
    Ok(ProbeOk {
        email: auth.email,
        plan: Some("Claude Subscription".into()),
        windows,
    })
}

fn push_claude_window(
    windows: &mut Vec<RuntimeUsageWindow>,
    id: &str,
    kind: &str,
    label: String,
    duration: u64,
    value: Option<&Value>,
) {
    let Some(window) = value.filter(|item| !item.is_null()) else {
        return;
    };
    let Some(used) = window.get("utilization").and_then(Value::as_f64) else {
        return;
    };
    windows.push(RuntimeUsageWindow {
        id: id.to_string(),
        kind: kind.to_string(),
        label,
        used_percent: clamp_percent(used),
        resets_at: window
            .get("resets_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        window_duration_mins: Some(duration),
    });
}

pub fn grok(home: &Path) -> Result<ProbeOk, ProbeError> {
    let auth = creds::grok_auth(home).ok_or(ProbeError::NoCredentials)?;
    let headers = vec![
        ("Authorization".into(), format!("Bearer {}", auth.token)),
        ("x-xai-token-auth".into(), "xai-grok-cli".into()),
        ("x-grok-client-version".into(), GROK_CLI_VERSION.into()),
        ("User-Agent".into(), format!("grok-cli/{GROK_CLI_VERSION}")),
    ];
    let usage = get_json(GROK_USAGE_URL, &headers)?;
    let config = usage.get("config");
    let period = config.and_then(|value| value.get("currentPeriod"));
    let period_type = period
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_start_matches("USAGE_PERIOD_TYPE_");
    let kind = match period_type {
        "WEEKLY" => "weekly",
        "MONTHLY" => "monthly",
        _ => "other",
    };
    let start = period
        .and_then(|value| value.get("start"))
        .and_then(Value::as_str)
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .map(|dt| dt.timestamp_millis());
    let end = period
        .and_then(|value| value.get("end"))
        .and_then(Value::as_str)
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok());
    let mut used = config
        .and_then(|value| value.get("creditUsagePercent"))
        .and_then(Value::as_f64);
    if used.is_none() && matches!(kind, "weekly" | "monthly") {
        used = Some(0.0);
    }
    let Some(used_percent) = used else {
        return Ok(ProbeOk {
            email: auth.email,
            plan: None,
            windows: Vec::new(),
        });
    };
    let window_duration_mins = match (start, end) {
        (Some(start_ms), Some(end_dt)) => {
            let mins = ((end_dt.timestamp_millis() - start_ms) as f64 / 60_000.0).round() as i64;
            (mins > 0).then_some(mins as u64)
        }
        _ => None,
    };
    Ok(ProbeOk {
        email: auth.email,
        plan: None,
        windows: vec![RuntimeUsageWindow {
            id: "subscription".into(),
            kind: kind.to_string(),
            label: label_for_kind(kind),
            used_percent: clamp_percent(used_percent),
            resets_at: end.map(|dt| dt.to_utc().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            window_duration_mins,
        }],
    })
}

pub fn cursor(home: &Path) -> Result<ProbeOk, ProbeError> {
    let auth = creds::cursor_auth(home).ok_or(ProbeError::NoCredentials)?;
    let headers = vec![
        ("Authorization".into(), format!("Bearer {}", auth.token)),
        ("Content-Type".into(), "application/json".into()),
        ("connect-protocol-version".into(), "1".into()),
        ("x-cursor-client-type".into(), "cli".into()),
    ];
    let usage = post_json(CURSOR_USAGE_URL, &headers, "{}")?;
    let plan = usage.get("planUsage");
    let cycle_start = usage
        .get("billingCycleStart")
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()));
    let cycle_end = usage
        .get("billingCycleEnd")
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()));
    let resets_at = cycle_end.and_then(iso_from_epoch_ms);
    let window_duration_mins = match (cycle_start, cycle_end) {
        (Some(start), Some(end)) if end > start => Some(((end - start) / 60_000.0).round() as u64),
        _ => None,
    };
    let mut windows = Vec::new();
    for (id, label_key) in [
        ("totalPercentUsed", tr!("总额度")),
        ("autoPercentUsed", tr!("Auto")),
        ("apiPercentUsed", tr!("API")),
    ] {
        let Some(plan_usage) = plan else {
            break;
        };
        let Some(used) = plan_usage.get(id).and_then(Value::as_f64) else {
            continue;
        };
        windows.push(RuntimeUsageWindow {
            id: id.to_string(),
            kind: "monthly".into(),
            label: label_key,
            used_percent: clamp_percent(used),
            resets_at: resets_at.clone(),
            window_duration_mins,
        });
    }
    Ok(ProbeOk {
        email: auth.email,
        plan: None,
        windows,
    })
}

pub fn opencode(home: &Path) -> Result<ProbeOk, ProbeError> {
    let key = creds::opencode_go_key(home).ok_or(ProbeError::NoCredentials)?;
    let headers = vec![("Authorization".into(), format!("Bearer {key}"))];
    let usage = get_json(OPENCODE_USAGE_URL, &headers)?;
    let rolling = usage.pointer("/usage/rolling");
    let weekly = usage.pointer("/usage/weekly");
    let monthly = usage.pointer("/usage/monthly");
    let mut windows = Vec::new();
    push_opencode_window(
        &mut windows,
        "go_rolling",
        "session",
        tr!("Go · 会话"),
        SESSION_MINS,
        rolling,
    );
    push_opencode_window(
        &mut windows,
        "go_weekly",
        "weekly",
        tr!("Go · 每周"),
        WEEK_MINS,
        weekly,
    );
    push_opencode_window(
        &mut windows,
        "go_monthly",
        "monthly",
        tr!("Go · 每月"),
        0,
        monthly,
    );
    Ok(ProbeOk {
        email: None,
        plan: Some("OpenCode Go".into()),
        windows,
    })
}

fn push_opencode_window(
    windows: &mut Vec<RuntimeUsageWindow>,
    id: &str,
    kind: &str,
    label: String,
    duration: u64,
    value: Option<&Value>,
) {
    let Some(window) = value else {
        return;
    };
    let Some(percent) = window.get("percent").and_then(Value::as_f64) else {
        return;
    };
    windows.push(RuntimeUsageWindow {
        id: id.to_string(),
        kind: kind.to_string(),
        label,
        used_percent: clamp_percent(percent),
        resets_at: window
            .get("resetsAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        window_duration_mins: (duration > 0).then_some(duration),
    });
}

pub fn gemini(home: &Path) -> Result<ProbeOk, ProbeError> {
    let auth = creds::gemini_oauth(home).ok_or(ProbeError::NoCredentials)?;
    let windows = cloudcode_quota_windows(&auth, GEMINI_USER_AGENT)?;
    Ok(ProbeOk {
        email: auth.email,
        plan: Some("Gemini".into()),
        windows,
    })
}

pub fn antigravity(home: &Path) -> Result<ProbeOk, ProbeError> {
    let auth = creds::antigravity_oauth(home).ok_or(ProbeError::NoCredentials)?;
    let windows = cloudcode_quota_windows(&auth, ANTIGRAVITY_USER_AGENT)?;
    Ok(ProbeOk {
        email: auth.email,
        plan: Some("Antigravity".into()),
        windows,
    })
}

fn cloudcode_quota_windows(
    auth: &creds::GoogleOAuth,
    user_agent: &str,
) -> Result<Vec<RuntimeUsageWindow>, ProbeError> {
    let body = match &auth.project_id {
        Some(project) if !project.is_empty() => format!(r#"{{"project":{}}}"#, json_string(project)),
        _ => "{}".to_string(),
    };
    let headers = vec![
        (
            "Authorization".into(),
            format!("Bearer {}", auth.access_token),
        ),
        ("Content-Type".into(), "application/json".into()),
        ("User-Agent".into(), user_agent.into()),
    ];
    let mut best: Vec<RuntimeUsageWindow> = Vec::new();
    let mut last_error = None;
    for base in [CLOUDCODE_DAILY, CLOUDCODE_PROD] {
        let url = format!("{base}/v1internal:retrieveUserQuotaSummary");
        match post_json(&url, &headers, &body) {
            Ok(summary) => {
                let windows = parse_cloudcode_summary(&summary);
                if scored_usage(&windows) >= scored_usage(&best) {
                    best = windows;
                }
            }
            Err(error) => last_error = Some(error),
        }
        // 旧账户可能只有 per-model 配额
        let models_url = format!("{base}/v1internal:retrieveUserQuota");
        if let Ok(models) = post_json(&models_url, &headers, &body) {
            let windows = parse_cloudcode_models(&models);
            if scored_usage(&windows) > scored_usage(&best) {
                best = windows;
            }
        }
    }
    if !best.is_empty() {
        return Ok(best);
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(Vec::new()),
    }
}

fn scored_usage(windows: &[RuntimeUsageWindow]) -> (usize, i64) {
    let used = windows
        .iter()
        .map(|window| window.used_percent.round() as i64)
        .sum::<i64>();
    (windows.len(), used)
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
pub(super) fn parse_cloudcode_summary_for_test(summary: &Value) -> Vec<RuntimeUsageWindow> {
    parse_cloudcode_summary(summary)
}

fn parse_cloudcode_summary(summary: &Value) -> Vec<RuntimeUsageWindow> {
    let mut windows = Vec::new();
    let groups = summary
        .get("groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !groups.is_empty() {
        for group in groups {
            let group_name = group
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Quota");
            let buckets = group
                .get("buckets")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for bucket in buckets {
                if let Some(window) = cloudcode_bucket_window(&bucket, group_name) {
                    windows.push(window);
                }
            }
        }
        return dedupe_windows(windows);
    }
    if let Some(buckets) = summary.get("buckets").and_then(Value::as_array) {
        for bucket in buckets {
            if let Some(window) = cloudcode_bucket_window(bucket, "Quota") {
                windows.push(window);
            }
        }
    }
    dedupe_windows(windows)
}

fn parse_cloudcode_models(models: &Value) -> Vec<RuntimeUsageWindow> {
    let mut windows = Vec::new();
    let Some(map) = models.get("models").and_then(Value::as_object) else {
        return windows;
    };
    for (model_id, model) in map {
        let family = if model_id.starts_with("gemini-")
            || model.get("modelProvider").and_then(Value::as_str) == Some("MODEL_PROVIDER_GOOGLE")
        {
            "Gemini"
        } else if model_id.starts_with("claude-") {
            "Claude"
        } else if model_id.starts_with("gpt-") {
            "GPT"
        } else {
            continue;
        };
        if let Some(quota) = model.get("quotaInfo") {
            if let Some(window) =
                cloudcode_quota_fields(quota, &format!("{family}-quota"), &tr!("{name} · 配额", name = family), "other", None)
            {
                windows.push(window);
            }
        }
        if let Some(quota) = model.get("weeklyQuotaInfo") {
            if let Some(window) = cloudcode_quota_fields(
                quota,
                &format!("{family}-weekly"),
                &tr!("{name} · 每周", name = family),
                "weekly",
                Some(WEEK_MINS),
            ) {
                windows.push(window);
            }
        }
    }
    dedupe_windows(windows)
}

fn cloudcode_bucket_window(bucket: &Value, group_name: &str) -> Option<RuntimeUsageWindow> {
    if bucket.get("disabled").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let bucket_id = bucket
        .get("bucketId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("quota");
    let window = bucket
        .get("window")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let (kind, duration, period_label) = match window {
        "5h" => ("session", Some(SESSION_MINS), tr!("5 小时")),
        "weekly" => ("weekly", Some(WEEK_MINS), tr!("每周")),
        _ => (
            "other",
            None,
            bucket
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| tr!("配额")),
        ),
    };
    let family = if bucket_id.starts_with("gemini-argon-") {
        tr!("Gemini Argon")
    } else if bucket_id.starts_with("gemini-carbon-") {
        tr!("Gemini Carbon")
    } else if bucket_id.starts_with("gemini-") {
        "Gemini".into()
    } else if bucket_id.starts_with("3p-") {
        tr!("Claude & GPT")
    } else {
        group_name.to_string()
    };
    let label = format!("{family} · {period_label}");
    cloudcode_quota_fields(bucket, bucket_id, &label, kind, duration)
}

fn cloudcode_quota_fields(
    quota: &Value,
    id: &str,
    label: &str,
    kind: &str,
    duration: Option<u64>,
) -> Option<RuntimeUsageWindow> {
    let remaining = quota.get("remainingFraction").and_then(Value::as_f64)?;
    if !remaining.is_finite() {
        return None;
    }
    let used = clamp_percent((1.0 - remaining.clamp(0.0, 1.0)) * 100.0);
    Some(RuntimeUsageWindow {
        id: id.to_string(),
        kind: kind.to_string(),
        label: label.to_string(),
        used_percent: used,
        resets_at: quota
            .get("resetTime")
            .and_then(Value::as_str)
            .map(str::to_string),
        window_duration_mins: duration,
    })
}

fn dedupe_windows(windows: Vec<RuntimeUsageWindow>) -> Vec<RuntimeUsageWindow> {
    let mut by_id = std::collections::BTreeMap::new();
    for window in windows {
        by_id
            .entry(window.id.clone())
            .and_modify(|existing: &mut RuntimeUsageWindow| {
                if window.used_percent > existing.used_percent {
                    *existing = window.clone();
                }
            })
            .or_insert(window);
    }
    by_id.into_values().collect()
}
