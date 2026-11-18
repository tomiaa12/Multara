//! JSON Schema 子集校验（type / properties / required / items / enum / const / additionalProperties /
//! minLength / maxLength / minimum / maximum / minItems / maxItems / anyOf / oneOf / allOf / pattern）。
//! 不支持 `$ref` 远程引用；`builtin:` 引用由 `resolve_builtin` 展开。错误带字段路径，给 agent 看得懂。

use serde_json::{json, Value};

pub fn resolve_builtin(schema: &Value) -> Value {
    if let Some(name) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(found) = builtin(name) {
            return found;
        }
    }
    schema.clone()
}

pub fn builtin(name: &str) -> Option<Value> {
    match name {
        "builtin:prd-spec" => Some(json!({
            "type": "object",
            "required": ["title", "goals", "acceptance"],
            "properties": {
                "title": { "type": "string", "minLength": 1 },
                "background": { "type": "string" },
                "goals": { "type": "array", "items": { "type": "string" } },
                "user_stories": { "type": "array", "items": {
                    "type": "object",
                    "properties": { "as": { "type": "string" }, "want": { "type": "string" }, "so": { "type": "string" } }
                } },
                "acceptance": { "type": "array", "items": { "type": "string" } },
                "out_of_scope": { "type": "array", "items": { "type": "string" } },
                "apis": { "type": "array" },
                "pages": { "type": "array" }
            }
        })),
        "builtin:review-verdict" => Some(json!({
            "type": "object",
            "required": ["pass", "issues"],
            "properties": {
                "pass": { "type": "boolean" },
                "issues": { "type": "array", "items": {
                    "type": "object",
                    "required": ["comment"],
                    "properties": {
                        "file": { "type": "string" }, "line": { "type": "integer" },
                        "severity": { "enum": ["blocker", "major", "minor", "nit"] },
                        "comment": { "type": "string" }
                    }
                } }
            }
        })),
        "builtin:string-list" => Some(json!({ "type": "array", "items": { "type": "string" } })),
        _ => None,
    }
}

pub fn validate(schema: &Value, value: &Value) -> Result<(), Vec<String>> {
    let schema = resolve_builtin(schema);
    let mut errors = Vec::new();
    check(&schema, value, "$", &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        errors.truncate(20);
        Err(errors)
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        _ => true,
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn check(schema: &Value, value: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(schema) = schema.as_object() else {
        return;
    };
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(found) = builtin(reference) {
            check(&found, value, path, errors);
        }
    }
    if let Some(expected) = schema.get("type") {
        let ok = match expected {
            Value::String(name) => type_matches(name, value),
            Value::Array(names) => names
                .iter()
                .filter_map(Value::as_str)
                .any(|name| type_matches(name, value)),
            _ => true,
        };
        if !ok {
            errors.push(format!(
                "{path}: expected {}, got {}",
                expected.to_string().trim_matches('"'),
                type_name(value)
            ));
            return;
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        if !options.contains(value) {
            errors.push(format!("{path}: must be one of {}", Value::Array(options.clone())));
        }
    }
    if let Some(expected) = schema.get("const") {
        if expected != value {
            errors.push(format!("{path}: must equal {expected}"));
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
            if length < min {
                errors.push(format!("{path}: shorter than {min}"));
            }
        }
        if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
            if length > max {
                errors.push(format!("{path}: longer than {max}"));
            }
        }
        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
            if let Ok(regex) = regex::Regex::new(pattern) {
                if !regex.is_match(text) {
                    errors.push(format!("{path}: does not match /{pattern}/"));
                }
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
            if number < min {
                errors.push(format!("{path}: less than {min}"));
            }
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
            if number > max {
                errors.push(format!("{path}: greater than {max}"));
            }
        }
    }
    if let Some(map) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !map.contains_key(key) {
                    errors.push(format!("{path}.{key}: is required"));
                }
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(properties) = properties {
            for (key, sub) in properties {
                if let Some(child) = map.get(key) {
                    check(sub, child, &format!("{path}.{key}"), errors);
                }
            }
        }
        match schema.get("additionalProperties") {
            Some(Value::Bool(false)) => {
                for key in map.keys() {
                    if !properties.is_some_and(|props| props.contains_key(key)) {
                        errors.push(format!("{path}.{key}: is not allowed"));
                    }
                }
            }
            Some(sub @ Value::Object(_)) => {
                for (key, child) in map {
                    if !properties.is_some_and(|props| props.contains_key(key)) {
                        check(sub, child, &format!("{path}.{key}"), errors);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(items) = value.as_array() {
        if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
            if (items.len() as u64) < min {
                errors.push(format!("{path}: fewer than {min} items"));
            }
        }
        if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
            if (items.len() as u64) > max {
                errors.push(format!("{path}: more than {max} items"));
            }
        }
        if let Some(sub) = schema.get("items") {
            for (i, item) in items.iter().enumerate() {
                check(sub, item, &format!("{path}[{i}]"), errors);
            }
        }
    }
    if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
        for sub in all {
            check(sub, value, path, errors);
        }
    }
    let count_ok = |subs: &Vec<Value>| {
        subs.iter()
            .filter(|sub| {
                let mut nested = Vec::new();
                check(sub, value, path, &mut nested);
                nested.is_empty()
            })
            .count()
    };
    if let Some(any) = schema.get("anyOf").and_then(Value::as_array) {
        if count_ok(any) == 0 {
            errors.push(format!("{path}: does not match any allowed shape"));
        }
    }
    if let Some(one) = schema.get("oneOf").and_then(Value::as_array) {
        if count_ok(one) != 1 {
            errors.push(format!("{path}: must match exactly one allowed shape"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_paths() {
        let schema = json!({
            "type": "object", "required": ["pass", "issues"],
            "properties": { "pass": { "type": "boolean" },
              "issues": { "type": "array", "items": { "type": "object", "required": ["comment"] } } }
        });
        assert!(validate(&schema, &json!({ "pass": true, "issues": [] })).is_ok());
        let errors = validate(&schema, &json!({ "pass": "yes", "issues": [{}] })).unwrap_err();
        assert!(errors.iter().any(|e| e.starts_with("$.pass")));
        assert!(errors.iter().any(|e| e.starts_with("$.issues[0].comment")));
    }

    #[test]
    fn builtin_refs() {
        let schema = json!({ "$ref": "builtin:prd-spec" });
        assert!(validate(&schema, &json!({ "title": "x", "goals": [], "acceptance": [] })).is_ok());
        assert!(validate(&schema, &json!({ "title": "" })).is_err());
    }

    #[test]
    fn combinators_and_enum() {
        let schema = json!({ "anyOf": [{ "type": "string" }, { "type": "integer" }] });
        assert!(validate(&schema, &json!(3)).is_ok());
        assert!(validate(&schema, &json!(true)).is_err());
        assert!(validate(&json!({ "enum": ["a"] }), &json!("b")).is_err());
    }
}
