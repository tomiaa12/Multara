//! 应用界面 / 内置提示词语言。与前端 `src/i18n` 的 Locale 对齐。

use std::sync::RwLock;

use rusqlite::Connection;

pub const LOCALES: &[&str] = &["zh-CN", "en", "zh-TW", "ja", "ko"];

/// 后端报错与 `tr!` 用的当前语言；启动、CLI 打开数据库、保存语言设置时刷新
static CURRENT: RwLock<&'static str> = RwLock::new("zh-CN");

pub fn current() -> &'static str {
    CURRENT.read().map(|locale| *locale).unwrap_or("zh-CN")
}

pub fn set_current(locale: &'static str) {
    if let Ok(mut current) = CURRENT.write() {
        *current = locale;
    }
}

/// 从设置值（含 `system`）解析为具体 locale id。
pub fn resolve(setting: &str) -> &'static str {
    let raw = if setting.is_empty() || setting == "system" {
        system_language()
    } else {
        setting.to_string()
    };
    normalize(&raw)
}

pub fn from_db(db: &Connection) -> &'static str {
    let setting = db
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'language'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "system".into());
    resolve(&setting)
}

pub fn is_supported_setting(value: &str) -> bool {
    value == "system" || LOCALES.contains(&value)
}

/// 设置里未自定义时，分支名生成默认提示词（与前端 `settings.gen.defaultBranchPrompt` 对齐）。
pub fn default_branch_prompt(locale: &str) -> &'static str {
    match locale {
        "en" => {
            "Suggest a short Git branch name from the requirement name. Do not paste the full title; extract 2–5 keywords. Prefer lowercase English with hyphens; keep proper nouns as-is. Output one line only."
        }
        "zh-TW" => {
            "根據需求名起一個簡短的 Git 分支名。不要把整段需求標題原樣拼進去，提煉 2～5 個關鍵詞。優先小寫英文和連字號，專有名詞可以保留原文。只輸出一行分支名。"
        }
        "ja" => {
            "要件名から短い Git ブランチ名を作ってください。タイトル全文をそのまま使わず、2〜5 個のキーワードに要約します。小文字の英語とハイフンを優先し、固有名詞はそのままで構いません。ブランチ名を一行だけ出力してください。"
        }
        "ko" => {
            "요구사항 이름으로 짧은 Git 브랜치명을 만드세요. 제목 전체를 그대로 쓰지 말고 키워드 2~5개로 요약하세요. 소문자 영어와 하이픈을 우선하고, 고유명사는 그대로 둬도 됩니다. 브랜치명만 한 줄로 출력하세요."
        }
        _ => {
            "根据需求名起一个简短的 Git 分支名。不要把整段需求标题原样拼进去，提炼 2～5 个关键词。优先小写英文和连字符，专有名词可以保留原文。只输出一行分支名。"
        }
    }
}

/// 设置里未自定义时，Commit 生成默认提示词（与前端 `settings.gen.defaultCommitPrompt` 对齐）。
pub fn default_commit_prompt(locale: &str) -> &'static str {
    match locale {
        "en" => {
            "Write one Conventional Commit message from the staged Git diff only. One line, under 100 characters, use feat/fix/refactor prefixes, no explanation."
        }
        "zh-TW" => {
            "只根據已暫存的 Git diff 寫一條 Conventional Commit 資訊。只輸出一行，不超過 100 個字元，使用 feat/fix/refactor 等前綴，不要解釋。"
        }
        "ja" => {
            "ステージ済みの Git diff だけから Conventional Commit メッセージを一行書いてください。100 文字以内、feat/fix/refactor などの接頭辞を使い、説明は付けないでください。"
        }
        "ko" => {
            "스테이징된 Git diff만 보고 Conventional Commit 메시지를 한 줄로 작성하세요. 100자 이내, feat/fix/refactor 등 접두사를 쓰고 설명은 하지 마세요."
        }
        _ => {
            "只根据已暂存的 Git diff 写一条 Conventional Commit 信息。只输出一行，不超过 100 个字符，使用 feat/fix/refactor 等前缀，不要解释。"
        }
    }
}

pub fn commit_diff_wrapper(locale: &str) -> &'static str {
    match locale {
        "en" => {
            "Write a commit message from the staged Git diff below only. Output the message alone — no explanation, no Markdown, no tools.\n\n"
        }
        "zh-TW" => {
            "只根據以下已暫存 Git diff 生成提交資訊。只輸出提交資訊，不要解釋，不要使用 Markdown，不要呼叫工具。\n\n"
        }
        "ja" => {
            "以下のステージ済み Git diff だけからコミットメッセージを生成してください。メッセージだけを出力し、説明・Markdown・ツール呼び出しは不要です。\n\n"
        }
        "ko" => {
            "아래 스테이징된 Git diff만으로 커밋 메시지를 작성하세요. 메시지만 출력하고, 설명·Markdown·도구 호출은 하지 마세요.\n\n"
        }
        _ => {
            "只根据以下已暂存 Git diff 生成提交信息。只输出提交信息，不要解释，不要使用 Markdown，不要调用工具。\n\n"
        }
    }
}

pub fn branch_name_wrapper(locale: &str, prefix: &str, requirement: &str) -> String {
    match locale {
        "en" => format!(
            "\n\nOutput one Git branch name only — no explanation, no Markdown, no tools. Default prefix is {prefix}.\n\nRequirement name: {requirement}"
        ),
        "zh-TW" => format!(
            "\n\n只輸出一行 Git 分支名，不要解釋，不要使用 Markdown，不要呼叫工具。預設前綴是 {prefix}。\n\n需求名：{requirement}"
        ),
        "ja" => format!(
            "\n\nGit ブランチ名を一行だけ出力し、説明・Markdown・ツール呼び出しは不要です。既定プレフィックスは {prefix}。\n\n要件名：{requirement}"
        ),
        "ko" => format!(
            "\n\nGit 브랜치명만 한 줄로 출력하세요. 설명·Markdown·도구 호출은 하지 마세요. 기본 접두사는 {prefix}입니다.\n\n요구사항 이름: {requirement}"
        ),
        _ => format!(
            "\n\n只输出一行 Git 分支名，不要解释，不要使用 Markdown，不要调用工具。默认前缀是 {prefix}。\n\n需求名：{requirement}"
        ),
    }
}

fn normalize(raw: &str) -> &'static str {
    let lower = raw.to_ascii_lowercase().replace('_', "-");
    if lower == "zh-tw" || lower == "zh-hk" || lower == "zh-mo" || lower.starts_with("zh-hant") {
        return "zh-TW";
    }
    if lower == "zh-cn" || lower.starts_with("zh") {
        return "zh-CN";
    }
    if lower == "ja" || lower.starts_with("ja-") {
        return "ja";
    }
    if lower == "ko" || lower.starts_with("ko-") {
        return "ko";
    }
    if lower == "en" || lower.starts_with("en-") {
        return "en";
    }
    "en"
}

fn system_language() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "en".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_common_tags() {
        assert_eq!(resolve("zh-CN"), "zh-CN");
        assert_eq!(resolve("en"), "en");
        assert_eq!(resolve("zh-TW"), "zh-TW");
        assert_eq!(resolve("ja"), "ja");
        assert_eq!(resolve("ko"), "ko");
        assert_eq!(normalize("zh_HK"), "zh-TW");
        assert_eq!(normalize("en-US"), "en");
        assert_eq!(normalize("ja_JP"), "ja");
    }
}
