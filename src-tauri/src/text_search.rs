use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
};

use regex::Regex;
use serde::Serialize;

const MATCH_LIMIT: usize = 2000;
const FILE_LIMIT: u64 = 1024 * 1024;
const PREVIEW_LIMIT: usize = 500;
/// 不跟随符号链接，再加深度上限兜底异常深的目录。
const MAX_SEARCH_DEPTH: usize = 64;

static ACTIVE_SEARCHES: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 同一个 key 同时只保留最新一次搜索：开始新搜索时把旧搜索标记为取消。
/// 守卫释放时只移除自己登记的那一项。
pub struct SearchTicket {
    key: String,
    cancelled: Arc<AtomicBool>,
}

impl SearchTicket {
    pub fn begin(key: &str) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let previous = active_searches().insert(key.to_string(), Arc::clone(&cancelled));
        if let Some(previous) = previous {
            previous.store(true, Ordering::Release);
        }
        Self {
            key: key.to_string(),
            cancelled,
        }
    }

    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

impl Drop for SearchTicket {
    fn drop(&mut self) {
        let mut active = active_searches();
        if active
            .get(&self.key)
            .is_some_and(|flag| Arc::ptr_eq(flag, &self.cancelled))
        {
            active.remove(&self.key);
        }
    }
}

fn active_searches() -> std::sync::MutexGuard<'static, HashMap<String, Arc<AtomicBool>>> {
    ACTIVE_SEARCHES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

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
    /// 置位后遍历尽快返回已找到的部分结果。
    pub cancelled: Option<Arc<AtomicBool>>,
}

struct GlobSet {
    include: Vec<GlobPattern>,
    exclude: Vec<GlobPattern>,
}

enum GlobPattern {
    /// 没有通配符时按文件夹前缀匹配，对应「在文件夹中查找」填入的 ./src/components。
    Folder(String),
    Wildcard {
        regex: Regex,
        whole_path: bool,
    },
}

impl GlobPattern {
    fn compile(pattern: &str) -> Option<Self> {
        let normalized = pattern.trim().trim_start_matches("./").trim_matches('/');
        if normalized.is_empty() {
            return None;
        }
        if !normalized.contains('*') && !normalized.contains('?') {
            return Some(Self::Folder(normalized.to_string()));
        }
        let regex = Regex::new(&format!("^{}$", glob_source(normalized))).ok()?;
        Some(Self::Wildcard {
            regex,
            whole_path: normalized.contains('/'),
        })
    }

    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Folder(folder) => {
                path == folder
                    || path
                        .strip_prefix(folder.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            }
            Self::Wildcard {
                regex,
                whole_path: true,
            } => regex.is_match(path),
            Self::Wildcard {
                regex,
                whole_path: false,
            } => regex.is_match(path.rsplit('/').next().unwrap_or(path)),
        }
    }
}

fn compile_patterns(raw: &str) -> Vec<GlobPattern> {
    raw.split(',').filter_map(GlobPattern::compile).collect()
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
        .map_err(|error| tr!("工作区不可用：{error}", error = error))?;
    let globs = GlobSet {
        include: compile_patterns(&query.include),
        exclude: compile_patterns(&query.exclude),
    };
    let mut matches = Vec::new();
    let mut truncated = false;
    walk(
        &root_path,
        query,
        &globs,
        &pattern,
        &mut matches,
        &mut truncated,
    );
    Ok(TextSearchResponse {
        matches,
        truncated,
        error: String::new(),
    })
}

/// 用 `ignore` 遍历：开启排除设置时同时遵守 .gitignore，并保留 `SEARCH_EXCLUDE_DIRS` 等静态排除。
fn walk(
    root: &Path,
    query: &SearchQuery,
    globs: &GlobSet,
    pattern: &Regex,
    matches: &mut Vec<TextSearchMatch>,
    truncated: &mut bool,
) {
    let use_excludes = query.use_exclude_settings;
    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .git_ignore(use_excludes)
        .git_global(use_excludes)
        .git_exclude(use_excludes)
        .parents(use_excludes)
        .follow_links(false)
        .max_depth(Some(MAX_SEARCH_DEPTH))
        .filter_entry(move |entry| {
            entry.depth() == 0
                || !use_excludes
                || !crate::files::hidden_in_search(&entry.file_name().to_string_lossy())
        })
        .build();
    for entry in walker.flatten() {
        if query
            .cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            *truncated = true;
            return;
        }
        if matches.len() >= MATCH_LIMIT {
            *truncated = true;
            return;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !file_included(&relative, use_excludes, globs) {
            continue;
        }
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() > FILE_LIMIT {
            continue;
        }
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if bytes.iter().take(1024).any(|byte| *byte == 0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        collect_matches(&relative, &text, pattern, matches, truncated);
        if *truncated {
            return;
        }
    }
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
    let preview_column =
        line[from..start].chars().count() as u32 + prefix.chars().count() as u32 + 1;
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

fn file_included(path: &str, use_excludes: bool, globs: &GlobSet) -> bool {
    if use_excludes && path.split('/').any(crate::files::hidden_in_search) {
        return false;
    }
    let included =
        globs.include.is_empty() || globs.include.iter().any(|pattern| pattern.matches(path));
    included && !globs.exclude.iter().any(|pattern| pattern.matches(path))
}

fn glob_source(pattern: &str) -> String {
    let mut source = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '*'
            && index + 2 < chars.len()
            && chars[index + 1] == '*'
            && chars[index + 2] == '/'
        {
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
    Regex::new(&pattern).map_err(|error| tr!("正则无效：{error}", error = error))
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
                cancelled: None,
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
                cancelled: None,
            },
        )
        .unwrap();
        assert_eq!(response.matches.len(), 1);
        assert_eq!(response.matches[0].path, "src/components/App.vue");
    }

    fn plain_query(cancelled: Option<Arc<AtomicBool>>) -> SearchQuery {
        SearchQuery {
            query: "Title".into(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            include: String::new(),
            exclude: "*.log".into(),
            use_exclude_settings: true,
            cancelled,
        }
    }

    #[test]
    fn honours_gitignore_and_does_not_follow_symlink_loops() {
        let root = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(root.path())
            .status()
            .unwrap();
        fs::write(root.path().join(".gitignore"), "dist/\n").unwrap();
        fs::create_dir(root.path().join("dist")).unwrap();
        fs::write(root.path().join("dist/out.js"), "Title\n").unwrap();
        fs::write(root.path().join("keep.ts"), "Title\n").unwrap();
        fs::write(root.path().join("skip.log"), "Title\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.path(), root.path().join("loop")).unwrap();
        let response = search_workspace(root.path().to_str().unwrap(), &plain_query(None)).unwrap();
        let paths: Vec<_> = response
            .matches
            .iter()
            .map(|item| item.path.as_str())
            .collect();
        assert_eq!(paths, ["keep.ts"]);
    }

    #[test]
    fn a_newer_search_with_the_same_key_cancels_the_older_one() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a.ts"), "Title\n").unwrap();
        let first = SearchTicket::begin("test-view");
        let second = SearchTicket::begin("test-view");
        let stale = search_workspace(
            root.path().to_str().unwrap(),
            &plain_query(Some(first.flag())),
        )
        .unwrap();
        assert!(stale.matches.is_empty() && stale.truncated);
        let fresh = search_workspace(
            root.path().to_str().unwrap(),
            &plain_query(Some(second.flag())),
        )
        .unwrap();
        assert_eq!(fresh.matches.len(), 1);
        drop(first);
        assert!(active_searches().contains_key("test-view"));
        drop(second);
        assert!(!active_searches().contains_key("test-view"));
    }
}
