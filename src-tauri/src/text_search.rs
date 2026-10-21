use std::{
    fs,
    path::Path,
};

use regex::Regex;
use serde::Serialize;

const MATCH_LIMIT: usize = 2000;
const FILE_LIMIT: u64 = 1024 * 1024;
const PREVIEW_LIMIT: usize = 500;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSearchMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub length: u32,
    pub preview: String,
    pub preview_column: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSearchResponse {
    pub matches: Vec<TextSearchMatch>,
    pub truncated: bool,
    pub error: String,
}

pub struct SearchQuery {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub include: String,
    pub exclude: String,
    pub use_exclude_settings: bool,
}

pub fn search_workspace(root: &str, query: &SearchQuery) -> Result<TextSearchResponse, String> {
    if query.query.trim().is_empty() {
        return Ok(TextSearchResponse {
            matches: Vec::new(),
            truncated: false,
            error: String::new(),
        });
    }
    let pattern = match build_pattern(query) {
        Ok(pattern) => pattern,
        Err(error) => {
            return Ok(TextSearchResponse {
                matches: Vec::new(),
                truncated: false,
                error,
            });
        }
    };
    let root_path = Path::new(root)
        .canonicalize()
        .map_err(|error| format!("工作区不可用：{error}"))?;
    let mut matches = Vec::new();
    let mut truncated = false;
    walk(&root_path, &root_path, query, &pattern, &mut matches, &mut truncated)?;
    Ok(TextSearchResponse {
        matches,
        truncated,
        error: String::new(),
    })
}

fn walk(
    root: &Path,
    directory: &Path,
    query: &SearchQuery,
    pattern: &Regex,
    matches: &mut Vec<TextSearchMatch>,
    truncated: &mut bool,
) -> Result<(), String> {
    if *truncated {
        return Ok(());
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        if matches.len() >= MATCH_LIMIT {
            *truncated = true;
            return Ok(());
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
            if query.use_exclude_settings && crate::files::hidden_in_search(&name) {
                continue;
            }
            walk(root, &path, query, pattern, matches, truncated)?;
            continue;
        }
        if !kind.is_file() || (query.use_exclude_settings && crate::files::hidden_in_search(&name)) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if !file_included(&relative, query) {
            continue;
        }
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() > FILE_LIMIT {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if bytes.iter().take(1024).any(|byte| *byte == 0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        collect_matches(&relative, &text, pattern, matches, truncated);
        if *truncated {
            return Ok(());
        }
    }
    Ok(())
}

fn collect_matches(
    path: &str,
    content: &str,
    pattern: &Regex,
    matches: &mut Vec<TextSearchMatch>,
    truncated: &mut bool,
) {
    for (index, line) in content.split('\n').enumerate() {
        let line = line.trim_end_matches('\r');
        for found in pattern.find_iter(line) {
            if matches.len() >= MATCH_LIMIT {
                *truncated = true;
                return;
            }
            let column = line[..found.start()].chars().count() as u32 + 1;
            let length = line[found.start()..found.end()].chars().count().max(1) as u32;
            let (preview, preview_column) = preview_window(line, found.start(), column);
            matches.push(TextSearchMatch {
                path: path.to_string(),
                line: (index + 1) as u32,
                column,
                length,
                preview,
                preview_column,
            });
        }
    }
}

fn preview_window(line: &str, start: usize, column: u32) -> (String, u32) {
    if line.chars().count() <= PREVIEW_LIMIT {
        return (line.to_string(), column);
    }
    let from = floor_char(line, start.saturating_sub(80));
    let end = ceil_char(line, (from + PREVIEW_LIMIT).min(line.len()));
    let prefix = if from > 0 { "…" } else { "" };
    let text = format!("{prefix}{}", &line[from..end]);
    let preview_column = line[from..start].chars().count() as u32 + prefix.chars().count() as u32 + 1;
    (text, preview_column)
}

fn floor_char(text: &str, mut index: usize) -> usize {
    if index > text.len() {
        index = text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char(text: &str, mut index: usize) -> usize {
    if index > text.len() {
        index = text.len();
    }
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn file_included(path: &str, query: &SearchQuery) -> bool {
    if query.use_exclude_settings && path.split('/').any(crate::files::hidden_in_search) {
        return false;
    }
    if !pattern_list_matches(path, &query.include, true) {
        return false;
    }
    if pattern_list_matches(path, &query.exclude, false) {
        return false;
    }
    true
}

fn pattern_list_matches(path: &str, raw: &str, empty_matches: bool) -> bool {
    let patterns: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect();
    if patterns.is_empty() {
        return empty_matches;
    }
    patterns.iter().any(|pattern| glob_match(path, pattern))
}

fn glob_match(path: &str, pattern: &str) -> bool {
    let normalized = pattern
        .trim()
        .trim_start_matches("./")
        .trim_matches('/');
    if normalized.is_empty() {
        return false;
    }
    // 没有通配符时按文件夹前缀匹配，对应「在文件夹中查找」填入的 ./src/components。
    if !normalized.contains('*') && !normalized.contains('?') {
        return path == normalized || path.starts_with(&format!("{normalized}/"));
    }
    let target = if normalized.contains('/') {
        path
    } else {
        path.rsplit('/').next().unwrap_or(path)
    };
    Regex::new(&format!("^{}$", glob_source(normalized)))
        .map(|regex| regex.is_match(target))
        .unwrap_or(false)
}

fn glob_source(pattern: &str) -> String {
    let mut source = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '*' && index + 2 < chars.len() && chars[index + 1] == '*' && chars[index + 2] == '/' {
            source.push_str("(?:^|.*/)");
            index += 3;
            continue;
        }
        if chars[index] == '*' && index + 1 < chars.len() && chars[index + 1] == '*' {
            source.push_str(".*");
            index += 2;
            continue;
        }
        match chars[index] {
            '*' => source.push_str("[^/]*"),
            '?' => source.push_str("[^/]"),
            char if ".+^${}()|[]\\".contains(char) => {
                source.push('\\');
                source.push(char);
            }
            char => source.push(char),
        }
        index += 1;
    }
    source
}

fn build_pattern(query: &SearchQuery) -> Result<Regex, String> {
    let source = if query.regex {
        query.query.clone()
    } else {
        regex::escape(&query.query)
    };
    let body = if !query.regex && query.whole_word {
        format!(r"\b(?:{source})\b")
    } else {
        source
    };
    let pattern = if query.case_sensitive {
        body
    } else {
        format!("(?i){body}")
    };
    Regex::new(&pattern).map_err(|error| format!("正则无效：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_line_and_skips_dependencies() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("App.vue"), "const name = \"Title\"\n").unwrap();
        fs::create_dir(root.path().join("node_modules")).unwrap();
        fs::write(root.path().join("node_modules").join("skip.txt"), "Title\n").unwrap();
        let response = search_workspace(
            root.path().to_str().unwrap(),
            &SearchQuery {
                query: "Title".into(),
                case_sensitive: false,
                whole_word: false,
                regex: false,
                include: "**/*.vue".into(),
                exclude: String::new(),
                use_exclude_settings: true,
            },
        )
        .unwrap();
        assert_eq!(response.matches.len(), 1);
        assert_eq!(response.matches[0].path, "App.vue");
        assert!(response.error.is_empty());
    }

    #[test]
    fn include_folder_matches_nested_files_only() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/components")).unwrap();
        fs::write(root.path().join("src/components/App.vue"), "Title\n").unwrap();
        fs::write(root.path().join("src/other.vue"), "Title\n").unwrap();
        let response = search_workspace(
            root.path().to_str().unwrap(),
            &SearchQuery {
                query: "Title".into(),
                case_sensitive: false,
                whole_word: false,
                regex: false,
                include: "./src/components".into(),
                exclude: String::new(),
                use_exclude_settings: true,
            },
        )
        .unwrap();
        assert_eq!(response.matches.len(), 1);
        assert_eq!(response.matches[0].path, "src/components/App.vue");
    }
}
