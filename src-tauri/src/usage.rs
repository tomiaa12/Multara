use serde_json::Value;

/// 一次运行解析到的 token / 费用。费用优先用 CLI 上报的数字，没有再按模型单价估算。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: f64,
    pub model: String,
    pub has_authoritative_cost: bool,
}

impl TokenUsage {
    pub fn total_tokens(&self) -> i64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_read_tokens
            + self.cache_write_tokens
    }

    fn is_empty(&self) -> bool {
        self.total_tokens() == 0 && self.cost_usd <= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UsageKind {
    Result,
    Step,
}

pub struct UsageAccumulator {
    configured_model: String,
    runtime_id: String,
    result: Option<TokenUsage>,
    steps: TokenUsage,
}

impl UsageAccumulator {
    pub fn new(configured_model: impl Into<String>, runtime_id: impl Into<String>) -> Self {
        Self {
            configured_model: configured_model.into(),
            runtime_id: runtime_id.into(),
            result: None,
            steps: TokenUsage::default(),
        }
    }

    #[cfg(test)]
    pub fn ingest(&mut self, line: &str) -> bool {
        mentions_usage(line)
            && serde_json::from_str::<Value>(line).is_ok_and(|value| self.ingest_value(&value))
    }

    /// 调用方已经解析好的行。先用 [`mentions_usage`] 过滤，避免每行都走一遍字段查找。
    pub fn ingest_value(&mut self, value: &Value) -> bool {
        let Some(parsed) = parse_value(value) else {
            return false;
        };
        let mut usage = parsed.usage;
        if usage.model.is_empty() {
            usage.model = self.configured_model.clone();
        }
        if self.runtime_id == "codex" {
            usage = normalize_codex_buckets(usage);
        }
        if parsed.kind == UsageKind::Result {
            self.result = Some(prefer_richer(self.result.take(), usage));
        } else {
            self.steps = add_usage(self.steps.clone(), usage);
        }
        true
    }

    pub fn finalize(&self) -> TokenUsage {
        let mut usage = self
            .result
            .clone()
            .filter(|item| !item.is_empty())
            .unwrap_or_else(|| self.steps.clone());
        if usage.model.is_empty() {
            usage.model = self.configured_model.clone();
        }
        if !usage.has_authoritative_cost {
            usage.cost_usd = estimate_cost(&usage, &self.runtime_id);
        }
        usage
    }
}

struct ParsedUsage {
    kind: UsageKind,
    usage: TokenUsage,
}

/// 廉价预过滤：用量字段名里总带 `usage` 或 `token`，不含的行不必看。
pub fn mentions_usage(line: &str) -> bool {
    line.contains("usage") || line.contains("token") || line.contains("Token")
}

fn parse_value(value: &Value) -> Option<ParsedUsage> {
    let usage = extract_usage(value)?;
    Some(ParsedUsage {
        kind: usage_kind(value),
        usage,
    })
}

fn usage_kind(value: &Value) -> UsageKind {
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    if matches!(
        event_type,
        "result" | "turn.completed" | "turn/completed" | "item.completed"
    ) {
        return UsageKind::Result;
    }
    if value.pointer("/params/turn/usage").is_some() {
        return UsageKind::Result;
    }
    UsageKind::Step
}

fn extract_usage(value: &Value) -> Option<TokenUsage> {
    let objects = [
        value.get("usage"),
        value.pointer("/message/usage"),
        value.pointer("/params/turn/usage"),
        value.pointer("/params/usage"),
        value.pointer("/payload/info/total_token_usage"),
        value.pointer("/payload/info/last_token_usage"),
        Some(value),
    ];
    for object in objects.into_iter().flatten() {
        if let Some(mut usage) = tokens_from_object(object) {
            usage.model = first_text(
                value,
                &[
                    "/model",
                    "/message/model",
                    "/params/turn/model",
                    "/payload/info/model",
                ],
            );
            if let Some(cost) = cost_from(value).or_else(|| cost_from(object)) {
                usage.cost_usd = cost;
                usage.has_authoritative_cost = cost > 0.0;
            }
            return Some(usage);
        }
    }
    None
}

fn tokens_from_object(object: &Value) -> Option<TokenUsage> {
    let input = int_field(
        object,
        &[
            "input_tokens",
            "inputTokens",
            "input",
            "prompt_tokens",
        ],
    );
    let output = int_field(
        object,
        &[
            "output_tokens",
            "outputTokens",
            "output",
            "completion_tokens",
        ],
    );
    let cache_read = int_field(
        object,
        &[
            "cache_read_tokens",
            "cacheReadTokens",
            "cached_input_tokens",
            "cachedInputTokens",
            "cache_read_input_tokens",
            "cacheReadInputTokens",
            "cached_input_tokens",
        ],
    );
    let cache_write = int_field(
        object,
        &[
            "cache_write_tokens",
            "cacheWriteTokens",
            "cache_creation_input_tokens",
            "cacheCreationInputTokens",
        ],
    );
    if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }
    Some(TokenUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        ..TokenUsage::default()
    })
}

fn cost_from(value: &Value) -> Option<f64> {
    if let Some(cost) = float_field(value, &["total_cost_usd", "cost_usd", "costUsd"]) {
        if cost > 0.0 {
            return Some(cost);
        }
    }
    let ticks = int_field(value, &["cost_usd_ticks", "costUsdTicks"]);
    if ticks > 0 {
        return Some(ticks as f64 / 10_000_000_000.0);
    }
    None
}

fn normalize_codex_buckets(mut usage: TokenUsage) -> TokenUsage {
    // Codex 的 input_tokens 含 cached_input_tokens，统计里拆成互斥桶。
    if usage.cache_read_tokens > 0 && usage.input_tokens >= usage.cache_read_tokens {
        usage.input_tokens -= usage.cache_read_tokens;
    }
    usage
}

fn add_usage(mut left: TokenUsage, right: TokenUsage) -> TokenUsage {
    left.input_tokens += right.input_tokens;
    left.output_tokens += right.output_tokens;
    left.cache_read_tokens += right.cache_read_tokens;
    left.cache_write_tokens += right.cache_write_tokens;
    if right.has_authoritative_cost {
        left.cost_usd += right.cost_usd;
        left.has_authoritative_cost = true;
    } else if !left.has_authoritative_cost {
        left.cost_usd += right.cost_usd;
    }
    if left.model.is_empty() {
        left.model = right.model;
    }
    left
}

fn prefer_richer(current: Option<TokenUsage>, next: TokenUsage) -> TokenUsage {
    match current {
        Some(old) if old.total_tokens() > next.total_tokens() => old,
        _ => next,
    }
}

fn int_field(value: &Value, names: &[&str]) -> i64 {
    for name in names {
        if let Some(number) = value.get(*name).and_then(json_i64) {
            if number != 0 {
                return number;
            }
        }
    }
    0
}

fn float_field(value: &Value, names: &[&str]) -> Option<f64> {
    for name in names {
        if let Some(number) = value.get(*name).and_then(json_f64) {
            return Some(number);
        }
    }
    None
}

fn first_text(value: &Value, pointers: &[&str]) -> String {
    for pointer in pointers {
        if let Some(text) = value.pointer(pointer).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

fn json_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|number| number as i64))
        .or_else(|| value.as_f64().map(|number| number as i64))
}

fn json_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|number| number as f64))
}

#[derive(Clone, Copy)]
struct Rates {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
}

/// 单价：美元 / 百万 token。未知模型估 0。
fn estimate_cost(usage: &TokenUsage, runtime_id: &str) -> f64 {
    let Some(rates) = resolve_pricing(&usage.model, runtime_id) else {
        return 0.0;
    };
    (usage.input_tokens as f64 * rates.input
        + usage.output_tokens as f64 * rates.output
        + usage.cache_read_tokens as f64 * rates.cache_read
        + usage.cache_write_tokens as f64 * rates.cache_write)
        / 1_000_000.0
}

fn resolve_pricing(model: &str, runtime_id: &str) -> Option<Rates> {
    let key = normalize_model(model);
    if key.is_empty() {
        return None;
    }
    let qualified = format!("{runtime_id}/{key}");
    pricing_exact(&qualified)
        .or_else(|| pricing_exact(&key))
        .or_else(|| pricing_family(&key))
}

fn normalize_model(model: &str) -> String {
    let mut text = model.trim().to_ascii_lowercase();
    if let Some(index) = text.find('[') {
        text.truncate(index);
        text = text.trim().to_string();
    }
    // 去掉末尾日期快照，如 -20250929
    if let Some((stem, suffix)) = text.rsplit_once('-') {
        if suffix.len() == 8 && suffix.chars().all(|ch| ch.is_ascii_digit()) {
            return stem.to_string();
        }
    }
    text
}

fn pricing_exact(key: &str) -> Option<Rates> {
    Some(match key {
        "claude-sonnet-5" => rates(2.0, 10.0, 0.20, 2.50),
        "claude-fable-5-1" | "claude-fable-5" => rates(10.0, 50.0, 0.25, 12.50),
        "claude-opus-5" => rates(5.0, 25.0, 0.50, 6.25),
        "claude-haiku-4-5" => rates(1.0, 5.0, 0.10, 1.25),
        "claude-sonnet-4-5" | "claude-sonnet-4-6" | "claude-sonnet-4" => {
            rates(3.0, 15.0, 0.30, 3.75)
        }
        "claude-opus-4-5" | "claude-opus-4-6" | "claude-opus-4-7" | "claude-opus-4-8" => {
            rates(5.0, 25.0, 0.50, 6.25)
        }
        "claude-opus-4-1" | "claude-opus-4" => rates(15.0, 75.0, 1.50, 18.75),
        "claude-haiku-3-5" => rates(0.80, 4.0, 0.08, 1.00),
        "gpt-5.6-sol" => rates(5.0, 30.0, 0.50, 6.25),
        "gpt-5.6-terra" => rates(2.50, 15.0, 0.25, 3.125),
        "gpt-5.6-luna" => rates(1.0, 6.0, 0.10, 1.25),
        "gpt-5.5" => rates(5.0, 30.0, 0.50, 5.0),
        "gpt-5.4-mini" => rates(0.75, 4.50, 0.075, 0.75),
        "gpt-5.4" => rates(2.50, 15.0, 0.25, 2.50),
        "gpt-5.3-codex" | "gpt-5-codex" | "gpt-5" => rates(1.25, 10.0, 0.125, 1.25),
        "gpt-5-mini" => rates(0.25, 2.0, 0.025, 0.25),
        "gpt-5-nano" => rates(0.05, 0.40, 0.005, 0.05),
        "gpt-4o-mini" => rates(0.15, 0.60, 0.075, 0.15),
        "gpt-4o" => rates(2.50, 10.0, 1.25, 2.50),
        "deepseek-chat" | "deepseek-reasoner" | "deepseek-v4-flash" => {
            rates(0.14, 0.28, 0.0028, 0.14)
        }
        "kimi-k2.6" => rates(0.95, 4.0, 0.16, 0.95),
        "kimi-k3" | "kimi/k3" => rates(3.0, 15.0, 0.30, 3.0),
        "qwen3.8-max" => rates(2.0, 6.0, 0.17, 2.5),
        "qwen3.7-plus" => rates(0.40, 1.60, 0.04, 0.50),
        "grok-4.6" => rates(2.0, 6.0, 0.50, 2.0),
        _ => return None,
    })
}

fn pricing_family(key: &str) -> Option<Rates> {
    if key.contains("claude") && key.contains("sonnet") {
        return Some(rates(3.0, 15.0, 0.30, 3.75));
    }
    if key.contains("claude") && key.contains("opus") {
        return Some(rates(5.0, 25.0, 0.50, 6.25));
    }
    if key.contains("claude") && key.contains("haiku") {
        return Some(rates(1.0, 5.0, 0.10, 1.25));
    }
    if key.contains("claude") && key.contains("fable") {
        return Some(rates(10.0, 50.0, 0.25, 12.50));
    }
    if key.starts_with("gpt-5.6-sol") {
        return Some(rates(5.0, 30.0, 0.50, 6.25));
    }
    if key.starts_with("gpt-5.4") {
        return Some(rates(2.50, 15.0, 0.25, 2.50));
    }
    if key.starts_with("gpt-5") {
        return Some(rates(1.25, 10.0, 0.125, 1.25));
    }
    if key.starts_with("gpt-4o") {
        return Some(rates(2.50, 10.0, 1.25, 2.50));
    }
    None
}

fn rates(input: f64, output: f64, cache_read: f64, cache_write: f64) -> Rates {
    Rates {
        input,
        output,
        cache_read,
        cache_write,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_claude_result_cost_over_step_sum() {
        let mut acc = UsageAccumulator::new("claude-sonnet-4-5", "claude");
        acc.ingest(
            r#"{"type":"assistant","message":{"model":"claude-sonnet-4-5","usage":{"input_tokens":100,"output_tokens":20}}}"#,
        );
        acc.ingest(
            r#"{"type":"result","total_cost_usd":0.012,"usage":{"input_tokens":1200,"output_tokens":80,"cache_read_input_tokens":400}}"#,
        );
        let usage = acc.finalize();
        assert_eq!(usage.input_tokens, 1200);
        assert_eq!(usage.output_tokens, 80);
        assert_eq!(usage.cache_read_tokens, 400);
        assert!((usage.cost_usd - 0.012).abs() < 1e-9);
        assert!(usage.has_authoritative_cost);
    }

    #[test]
    fn prefilter_skips_lines_without_usage_keywords() {
        assert!(!mentions_usage(r#"{"type":"assistant","message":{"content":"hi"}}"#));
        assert!(mentions_usage(r#"{"usage":{"input_tokens":1}}"#));
        assert!(mentions_usage(r#"{"inputTokens":1}"#));
        let mut acc = UsageAccumulator::new("gpt-5", "codex");
        assert!(!acc.ingest(r#"{"type":"text","text":"no numbers here"}"#));
    }

    #[test]
    fn estimates_cursor_step_usage_when_no_result() {
        let mut acc = UsageAccumulator::new("claude-4.5-sonnet", "cursor");
        acc.ingest(
            r#"{"type":"step_finish","usage":{"input_tokens":1000,"output_tokens":200,"cached_input_tokens":100}}"#,
        );
        let usage = acc.finalize();
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.cache_read_tokens, 100);
        assert!(usage.cost_usd > 0.0);
    }

    #[test]
    fn splits_codex_cached_input_out_of_prompt_tokens() {
        let mut acc = UsageAccumulator::new("gpt-5.4", "codex");
        acc.ingest(
            r#"{"type":"turn/completed","params":{"turn":{"usage":{"input_tokens":1000,"cached_input_tokens":300,"output_tokens":50}}}}"#,
        );
        let usage = acc.finalize();
        assert_eq!(usage.input_tokens, 700);
        assert_eq!(usage.cache_read_tokens, 300);
        assert_eq!(usage.output_tokens, 50);
        assert!(usage.cost_usd > 0.0);
    }
}
