//! 模板与表达式：MiniJinja，严格未定义。见技术方案 2.3。

use std::collections::{BTreeMap, BTreeSet};

use minijinja::{Environment, Error, ErrorKind, UndefinedBehavior, Value as JValue};
use serde_json::Value;

/// 模板用在哪里：决定能不能用 `secret()`、插值要不要 shell 转义
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    /// prompt、标题、分支名等普通文本
    Text,
    /// script 命令：插值按 shell 转义
    Command(ShellKind),
    /// script env / http url、headers、body：允许 secret()
    Secret,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShellKind {
    Posix,
    PowerShell,
    Cmd,
}

impl ShellKind {
    pub fn from_config(shell: &str) -> ShellKind {
        match shell {
            "pwsh" | "powershell" => ShellKind::PowerShell,
            "cmd" => ShellKind::Cmd,
            "sh" | "bash" => ShellKind::Posix,
            _ => {
                if cfg!(windows) {
                    ShellKind::PowerShell
                } else {
                    ShellKind::Posix
                }
            }
        }
    }
}

pub fn shell_quote(value: &str, shell: ShellKind) -> Result<String, String> {
    match shell {
        ShellKind::Posix => Ok(format!("'{}'", value.replace('\'', "'\\''"))),
        ShellKind::PowerShell => Ok(format!("'{}'", value.replace('\'', "''"))),
        ShellKind::Cmd => {
            if value
                .chars()
                .any(|c| matches!(c, '&' | '|' | '<' | '>' | '^' | '%' | '"' | '\n' | '\r' | '!'))
            {
                Err(tr!("cmd 命令中的插值包含特殊字符，请改用环境变量传递"))
            } else {
                Ok(format!("\"{value}\""))
            }
        }
    }
}

fn value_text(value: &JValue) -> String {
    if value.is_undefined() || value.is_none() {
        return String::new();
    }
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        Ok(other) if other.is_object() || other.is_array() => other.to_string(),
        Ok(other) => other.to_string(),
        Err(_) => value.to_string(),
    }
}

fn env_for(place: Place, secrets: Option<&BTreeMap<String, String>>) -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_keep_trailing_newline(true);
    env.add_filter("fromjson", |text: String| -> Result<JValue, Error> {
        let parsed: Value = serde_json::from_str(&text)
            .map_err(|error| Error::new(ErrorKind::InvalidOperation, error.to_string()))?;
        Ok(JValue::from_serialize(&parsed))
    });
    env.add_filter("truncate", |value: JValue, length: Option<usize>| -> String {
        let text = value_text(&value);
        let length = length.unwrap_or(255);
        if text.chars().count() <= length {
            text
        } else {
            let mut cut: String = text.chars().take(length).collect();
            cut.push('…');
            cut
        }
    });
    env.add_filter("jsonpath", |value: JValue, path: String| -> Result<JValue, Error> {
        let json = serde_json::to_value(&value)
            .map_err(|error| Error::new(ErrorKind::InvalidOperation, error.to_string()))?;
        Ok(JValue::from_serialize(json_path(&json, &path).unwrap_or(&Value::Null)))
    });
    env.add_filter("raw_unsafe", |value: JValue| -> JValue {
        JValue::from_safe_string(value_text(&value))
    });
    let shell = match place {
        Place::Command(shell) => shell,
        _ => ShellKind::Posix,
    };
    env.add_filter("shell_quote", move |value: JValue| -> Result<JValue, Error> {
        let quoted = shell_quote(&value_text(&value), shell)
            .map_err(|message| Error::new(ErrorKind::InvalidOperation, message))?;
        Ok(JValue::from_safe_string(quoted))
    });
    if let Place::Command(shell) = place {
        env.set_formatter(move |out, _state, value| {
            let text = value_text(value);
            if value.is_safe() {
                out.write_str(&text).map_err(Error::from)
            } else {
                let quoted = shell_quote(&text, shell)
                    .map_err(|message| Error::new(ErrorKind::InvalidOperation, message))?;
                out.write_str(&quoted).map_err(Error::from)
            }
        });
    } else {
        env.set_formatter(|out, _state, value| out.write_str(&value_text(value)).map_err(Error::from));
    }
    if place == Place::Secret {
        let secrets = secrets.cloned().unwrap_or_default();
        env.add_function("secret", move |name: String| -> Result<String, Error> {
            secrets.get(&name).cloned().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidOperation,
                    format!("secret {name} is not configured"),
                )
            })
        });
    }
    env
}

fn clean_error(error: Error) -> String {
    let mut message = error.to_string();
    if let Some(detail) = error.detail() {
        if !message.contains(detail) {
            message = format!("{message}: {detail}");
        }
    }
    message
}

/// 渲染成字符串
pub fn render(
    source: &str,
    context: &Value,
    place: Place,
    secrets: Option<&BTreeMap<String, String>>,
) -> Result<String, String> {
    if !source.contains("{{") && !source.contains("{%") {
        return Ok(source.to_string());
    }
    let env = env_for(place, secrets);
    env.render_str(source, JValue::from_serialize(context))
        .map_err(clean_error)
}

/// 求值表达式（switch 条件、transform、映射）
pub fn eval(source: &str, context: &Value) -> Result<Value, String> {
    let env = env_for(Place::Text, None);
    let expression = env.compile_expression(source).map_err(clean_error)?;
    let result = expression
        .eval(JValue::from_serialize(context))
        .map_err(clean_error)?;
    if result.is_undefined() {
        return Ok(Value::Null);
    }
    serde_json::to_value(&result).map_err(|error| error.to_string())
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// 整串只有一个 `{{ 表达式 }}` 时保留原类型，否则渲染成字符串
pub fn render_value(source: &str, context: &Value) -> Result<Value, String> {
    let trimmed = source.trim();
    if let Some(inner) = trimmed
        .strip_prefix("{{")
        .and_then(|rest| rest.strip_suffix("}}"))
    {
        if !inner.contains("{{") && !inner.contains("}}") && !inner.contains("{%") {
            return eval(inner.trim(), context);
        }
    }
    render(source, context, Place::Text, None).map(Value::String)
}

/// 语法检查
pub fn check_syntax(source: &str, is_expression: bool) -> Result<(), String> {
    let env = env_for(Place::Secret, None);
    if is_expression {
        env.compile_expression(source).map(|_| ()).map_err(clean_error)
    } else {
        env.template_from_str(source).map(|_| ()).map_err(clean_error)
    }
}

/// 引用到的变量路径（如 `nodes.prd.outputs.spec`）
pub fn referenced_paths(source: &str, is_expression: bool) -> BTreeSet<String> {
    let wrapped;
    let template_source = if is_expression {
        wrapped = format!("{{{{ {source} }}}}");
        wrapped.as_str()
    } else {
        source
    };
    let env = env_for(Place::Secret, None);
    match env.template_from_str(template_source) {
        Ok(template) => template.undeclared_variables(true).into_iter().collect(),
        Err(_) => BTreeSet::new(),
    }
}

pub fn uses_secret(source: &str) -> bool {
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("secret(")
}

/// 极简 JSONPath：`$.a.b[0].c`
pub fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim().trim_start_matches('$');
    let mut current = value;
    let mut token = String::new();
    let mut chars = path.chars().peekable();
    let step = |current: &'a Value, token: &str| -> Option<&'a Value> {
        if token.is_empty() {
            return Some(current);
        }
        match current {
            Value::Object(map) => map.get(token),
            Value::Array(items) => token.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                current = step(current, &token)?;
                token.clear();
            }
            '[' => {
                current = step(current, &token)?;
                token.clear();
                let mut inner = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    inner.push(c);
                }
                let inner = inner.trim_matches(|c| c == '\'' || c == '"');
                current = step(current, inner)?;
            }
            _ => token.push(c),
        }
    }
    step(current, &token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> Value {
        json!({
            "inputs": { "name": "it's", "count": 3, "spec": { "title": "登录" } },
            "nodes": { "prd": { "outputs": { "spec": { "title": "登录" } } } },
            "iteration": 1
        })
    }

    #[test]
    fn renders_and_is_strict() {
        assert_eq!(
            render("hi {{ inputs.spec.title }}", &ctx(), Place::Text, None).unwrap(),
            "hi 登录"
        );
        assert!(render("{{ inputs.missing }}", &ctx(), Place::Text, None).is_err());
        assert_eq!(
            render("{{ inputs.spec | tojson }}", &ctx(), Place::Text, None).unwrap(),
            "{\"title\":\"登录\"}"
        );
    }

    #[test]
    fn command_interpolation_is_quoted() {
        let out = render(
            "echo {{ inputs.name }} {{ inputs.name | raw_unsafe }}",
            &ctx(),
            Place::Command(ShellKind::Posix),
            None,
        )
        .unwrap();
        assert_eq!(out, "echo 'it'\\''s' it's");
        let out = render(
            "echo {{ inputs.name }}",
            &ctx(),
            Place::Command(ShellKind::PowerShell),
            None,
        )
        .unwrap();
        assert_eq!(out, "echo 'it''s'");
        assert!(shell_quote("a&b", ShellKind::Cmd).is_err());
    }

    #[test]
    fn secret_only_in_secret_place() {
        let mut secrets = BTreeMap::new();
        secrets.insert("TOKEN".to_string(), "s3".to_string());
        assert_eq!(
            render("Bearer {{ secret('TOKEN') }}", &ctx(), Place::Secret, Some(&secrets)).unwrap(),
            "Bearer s3"
        );
        assert!(render("{{ secret('TOKEN') }}", &ctx(), Place::Text, Some(&secrets)).is_err());
        assert!(uses_secret("{{ secret ('X') }}"));
    }

    #[test]
    fn expressions_keep_types() {
        assert_eq!(eval("inputs.count + 1", &ctx()).unwrap(), json!(4));
        assert!(truthy(&eval("inputs.count == 3", &ctx()).unwrap()));
        assert_eq!(
            render_value("{{ inputs.spec }}", &ctx()).unwrap(),
            json!({ "title": "登录" })
        );
        assert_eq!(
            render_value("x{{ inputs.count }}", &ctx()).unwrap(),
            json!("x3")
        );
    }

    #[test]
    fn finds_references() {
        let paths = referenced_paths("{{ nodes.prd.outputs.spec.title }} {{ inputs.name }}", false);
        assert!(paths.iter().any(|p| p.starts_with("nodes.prd")));
        assert!(paths.iter().any(|p| p.starts_with("inputs.name")));
        let paths = referenced_paths("inputs.value.exit_code == 0", true);
        assert!(paths.iter().any(|p| p.starts_with("inputs.value")));
    }

    #[test]
    fn json_path_works() {
        let value = json!({ "a": { "b": [ { "c": 1 } ] } });
        assert_eq!(json_path(&value, "$.a.b[0].c"), Some(&json!(1)));
        assert_eq!(json_path(&value, "$.a.x"), None);
    }
}
