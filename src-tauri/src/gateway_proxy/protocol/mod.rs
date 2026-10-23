use serde_json::Value;

pub(super) const RESPONSES_MIN_OUTPUT_TOKENS: u64 = 16;

pub(super) fn is_reasoning_model(model: &str) -> bool {
    let normalized = model.to_ascii_lowercase();
    normalized
        .strip_prefix('o')
        .and_then(|rest| rest.chars().next())
        .is_some_and(|value| value.is_ascii_digit())
        || normalized.starts_with("gpt-5")
        || normalized.starts_with("gpt-6")
}

pub(super) fn clean_schema(mut value: Value) -> Value {
    match &mut value {
        Value::Object(object) => {
            object.remove("$schema");
            if matches!(
                object.get("format").and_then(Value::as_str),
                Some("uri" | "uri-reference")
            ) {
                object.remove("format");
            }
            for child in object.values_mut() {
                *child = clean_schema(child.take());
            }
        }
        Value::Array(items) => {
            for item in items {
                *item = clean_schema(item.take());
            }
        }
        _ => {}
    }
    value
}
