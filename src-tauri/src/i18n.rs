//! 后端文案翻译：以简体中文原文为键，查 `src-tauri/locales/*.json`；没收录的原样返回原文。
//!
//! 用法：`tr!` 的第一个参数是中文原文字面量；带参数时原文里写 `{name}` 占位，后面跟 `name = 值`。
//! 每个 json 是 `{ "原文": { "en": "...", "zh-TW": "...", "ja": "...", "ko": "..." } }`。

use std::{collections::HashMap, fmt::Display, sync::OnceLock};

/// 按模块拆开，避免多人同时改一个大文件；新增文件要加到这里
const CATALOGS: &[(&str, &str)] = &[
    ("core", include_str!("../locales/core.json")),
    ("commands", include_str!("../locales/commands.json")),
    ("executor", include_str!("../locales/executor.json")),
    ("resources", include_str!("../locales/resources.json")),
    ("cli_ops", include_str!("../locales/cli_ops.json")),
    ("git", include_str!("../locales/git.json")),
    ("files", include_str!("../locales/files.json")),
    ("channel", include_str!("../locales/channel.json")),
];

type Catalog = HashMap<String, HashMap<String, String>>;

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        CATALOGS
            .iter()
            .filter_map(|(_, raw)| serde_json::from_str::<Catalog>(raw).ok())
            .flatten()
            .collect()
    })
}

pub fn tr(source: &str) -> String {
    tr_in(crate::app_locale::current(), source)
}

pub fn tr_in(locale: &str, source: &str) -> String {
    if locale == "zh-CN" {
        return source.to_string();
    }
    catalog()
        .get(source)
        .and_then(|entry| entry.get(locale))
        .filter(|text| !text.is_empty())
        .cloned()
        .unwrap_or_else(|| source.to_string())
}

pub fn trf(source: &str, args: &[(&str, &dyn Display)]) -> String {
    fill(tr(source), args)
}

fn fill(text: String, args: &[(&str, &dyn Display)]) -> String {
    args.iter().fold(text, |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), &value.to_string())
    })
}

#[macro_export]
macro_rules! tr {
    ($source:literal $(,)?) => {
        $crate::i18n::tr($source)
    };
    ($source:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::trf(
            $source,
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const TARGETS: &[&str] = &["en", "zh-TW", "ja", "ko"];

    fn placeholders(text: &str) -> BTreeSet<String> {
        regex::Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")
            .unwrap()
            .captures_iter(text)
            .map(|capture| capture[1].to_string())
            .collect()
    }

    /// 只认普通字符串字面量（不支持 raw string），按 Rust 规则反转义
    fn unescape(literal: &str) -> String {
        let mut out = String::new();
        let mut chars = literal.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some('\n') => {
                    while chars.peek().is_some_and(|next| next.is_whitespace()) {
                        chars.next();
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }
        out
    }

    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn catalogs_parse_and_cover_every_locale() {
        for (name, raw) in CATALOGS {
            let parsed: Catalog = serde_json::from_str(raw)
                .unwrap_or_else(|error| panic!("locales/{name}.json 解析失败：{error}"));
            for (source, entry) in parsed {
                for locale in TARGETS {
                    let text = entry
                        .get(*locale)
                        .unwrap_or_else(|| panic!("{name}: 「{source}」缺少 {locale}"));
                    assert!(!text.trim().is_empty(), "{name}: 「{source}」的 {locale} 为空");
                    assert_eq!(
                        placeholders(text),
                        placeholders(&source),
                        "{name}: 「{source}」的 {locale} 占位符不一致"
                    );
                }
            }
        }
    }

    #[test]
    fn every_tr_call_is_in_a_catalog() {
        let pattern = regex::Regex::new(r#"(?s)(?:^|[^A-Za-z0-9_])tr!\(\s*"((?:[^"\\]|\\.)*)""#).unwrap();
        let mut files = Vec::new();
        rust_files(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let mut missing = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for capture in pattern.captures_iter(&text) {
                let source = unescape(&capture[1]);
                if !catalog().contains_key(&source) {
                    missing.push(format!("{}: {source}", file.display()));
                }
            }
        }
        assert!(missing.is_empty(), "以下 tr! 文案没有翻译：\n{}", missing.join("\n"));
    }

    #[test]
    fn falls_back_to_source_and_fills_placeholders() {
        assert_eq!(tr_in("en", "这条文案不存在"), "这条文案不存在");
        assert_eq!(tr_in("zh-CN", "任意"), "任意");
        let value = 3;
        assert_eq!(
            fill("共 {n} 项".to_string(), &[("n", &value as &dyn Display)]),
            "共 3 项"
        );
    }
}
