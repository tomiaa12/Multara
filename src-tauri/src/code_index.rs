use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use ignore::WalkBuilder;
use serde::Serialize;
use serde_json::{Map, Value};

const MAX_SOURCE_FILES: usize = 3_000;
const MAX_SOURCE_BYTES: u64 = 512 * 1024;
const MAX_SOURCE_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TYPING_FILES: usize = 4_000;
const MAX_TYPING_TOTAL_BYTES: u64 = 24 * 1024 * 1024;
const MAX_TYPING_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PACKAGE_TYPINGS: usize = 600;
const MAX_DEPTH: usize = 16;
/// 构建产物里的 .js/.d.ts 会和源码重复声明，跳转时出现一堆无用候选。
const BUILD_DIRS: &[&str] = &[
    "dist", "build", "out", "out-build", "target", ".next", ".nuxt", ".output", "coverage", ".turbo", ".vite", ".cache", ".codeoss-build", ".pnpm-store",
];
const DEPENDENCY_FIELDS: &[&str] = &["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"];
const TRANSITIVE_FIELDS: &[&str] = &["dependencies", "peerDependencies"];
const MANIFEST_KEYS: &[&str] = &["name", "version", "types", "typings", "main", "module", "typesVersions"];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub path: String,
    pub mtime: u64,
    /// 与调用方已知的 mtime 相同时省略，避免每次刷新都重传整个工作区。
    pub content: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBatch {
    pub files: Vec<SourceFile>,
    pub truncated: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypingFile {
    /// 按 npm 扁平布局摆放的虚拟路径，TS 的 node_modules 查找只认这种布局。
    pub path: String,
    /// 磁盘上的真实位置（相对工作区）；pnpm 下与 `path` 不同，在工作区外时为空。
    pub disk_path: Option<String>,
    pub content: String,
}

pub fn collect_sources(
    root: &str,
    extensions: &[String],
    known: &HashMap<String, u64>,
) -> Result<SourceBatch, String> {
    let root = canonical(root)?;
    let extensions: HashSet<String> = extensions
        .iter()
        .map(|item| item.trim_start_matches('.').to_ascii_lowercase())
        .collect();
    let mut batch = SourceBatch { files: Vec::new(), truncated: false };
    let mut total = 0;
    let walker = WalkBuilder::new(&root)
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(MAX_DEPTH))
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !crate::files::hidden_in_search(&name) && !BUILD_DIRS.contains(&name.as_ref())
        })
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file())
            || !has_extension(&entry.file_name().to_string_lossy(), &extensions)
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > MAX_SOURCE_BYTES {
            batch.truncated = true;
            continue;
        }
        if batch.files.len() >= MAX_SOURCE_FILES {
            batch.truncated = true;
            break;
        }
        if total + metadata.len() > MAX_SOURCE_TOTAL_BYTES {
            batch.truncated = true;
            continue;
        }
        let path = entry.path();
        let relative = relative_path(&root, path);
        let mtime = modified_ms(&metadata);
        let content = if known.get(&relative) == Some(&mtime) {
            None
        } else {
            match read_limited(path, MAX_SOURCE_BYTES.min(MAX_SOURCE_TOTAL_BYTES - total)) {
                Some(text) => Some(text),
                None => continue,
            }
        };
        total += content.as_ref().map_or(metadata.len(), |text| text.len() as u64);
        batch.files.push(SourceFile { path: relative, mtime, content });
    }
    Ok(batch)
}

fn read_limited(path: &Path, limit: u64) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > limit {
        return None;
    }
    String::from_utf8(bytes).ok()
}

pub fn collect_typings(root: &str) -> Result<Vec<TypingFile>, String> {
    let root = canonical(root)?;
    let Some(manifest) = read_manifest(&root) else {
        return Ok(Vec::new());
    };
    let mut queue: VecDeque<(String, PathBuf)> = VecDeque::new();
    for name in dependency_names(&manifest, DEPENDENCY_FIELDS) {
        queue.push_back((name, root.clone()));
    }
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    let mut total = 0u64;
    while let Some((name, from)) = queue.pop_front() {
        if output.len() >= MAX_TYPING_FILES || total >= MAX_TYPING_TOTAL_BYTES {
            break;
        }
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(directory) = resolve_package(&from, &name) else {
            continue;
        };
        let Ok(real) = directory.canonicalize() else {
            continue;
        };
        let Some(package) = read_manifest(&real) else {
            continue;
        };
        let virtual_root = format!("node_modules/{name}");
        let manifest_text = trimmed_manifest(&package);
        if total + manifest_text.len() as u64 > MAX_TYPING_TOTAL_BYTES {
            break;
        }
        total += manifest_text.len() as u64;
        output.push(TypingFile {
            path: format!("{virtual_root}/package.json"),
            disk_path: disk_path(&root, &real.join("package.json")),
            content: manifest_text,
        });
        let mut declarations = Vec::new();
        walk_declarations(&real, &real, 0, &mut declarations);
        for path in &declarations {
            if output.len() >= MAX_TYPING_FILES || total >= MAX_TYPING_TOTAL_BYTES {
                break;
            }
            let Some(content) = read_limited(path, MAX_TYPING_FILE_BYTES.min(MAX_TYPING_TOTAL_BYTES - total)) else {
                continue;
            };
            total += content.len() as u64;
            output.push(TypingFile {
                path: format!("{virtual_root}/{}", relative_path(&real, path)),
                disk_path: disk_path(&root, path),
                content,
            });
        }
        if declarations.is_empty() && !name.starts_with("@types/") {
            queue.push_back((types_package_name(&name), root.clone()));
        }
        for dependency in dependency_names(&package, TRANSITIVE_FIELDS) {
            queue.push_back((dependency, real.clone()));
        }
    }
    Ok(output)
}

fn walk_declarations(root: &Path, directory: &Path, depth: usize, output: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || output.len() >= MAX_PACKAGE_TYPINGS {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if output.len() >= MAX_PACKAGE_TYPINGS {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if name == "node_modules" || name.starts_with('.') {
                continue;
            }
            walk_declarations(root, &entry.path(), depth + 1, output);
            continue;
        }
        if !is_declaration(&name) {
            continue;
        }
        if entry.metadata().map(|meta| meta.len() > MAX_TYPING_FILE_BYTES).unwrap_or(true) {
            continue;
        }
        output.push(entry.path());
    }
}

/// Node 的查找规则：从 `from` 起逐级向上找 `node_modules/<name>`。
/// pnpm 的依赖装在 `.pnpm/<pkg>/node_modules` 下彼此相邻，从真实目录出发才能找到。
fn resolve_package(from: &Path, name: &str) -> Option<PathBuf> {
    for directory in from.ancestors() {
        let candidate = directory.join("node_modules").join(name);
        if candidate.join("package.json").is_file() {
            return Some(candidate);
        }
    }
    None
}

fn read_manifest(directory: &Path) -> Option<Value> {
    let text = read_limited(&directory.join("package.json"), MAX_TYPING_FILE_BYTES)?;
    serde_json::from_str(&text).ok()
}

fn dependency_names(manifest: &Value, fields: &[&str]) -> Vec<String> {
    let mut names = Vec::new();
    for field in fields {
        if let Some(map) = manifest.get(*field).and_then(Value::as_object) {
            for name in map.keys() {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
    }
    names
}

fn trimmed_manifest(manifest: &Value) -> String {
    let mut trimmed = Map::new();
    for key in MANIFEST_KEYS {
        if let Some(value) = manifest.get(*key) {
            trimmed.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(trimmed).to_string()
}

fn types_package_name(name: &str) -> String {
    match name.strip_prefix('@') {
        Some(scoped) => format!("@types/{}", scoped.replacen('/', "__", 1)),
        None => format!("@types/{name}"),
    }
}

fn is_declaration(name: &str) -> bool {
    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
}

fn has_extension(name: &str, extensions: &HashSet<String>) -> bool {
    name.rsplit_once('.')
        .map(|(_, extension)| extensions.contains(&extension.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn canonical(root: &str) -> Result<PathBuf, String> {
    Path::new(root)
        .canonicalize()
        .map_err(|error| format!("工作区不可用：{error}"))
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn disk_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn modified_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn sources_skip_dependencies_and_unchanged_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("src/a.ts"), "export const a = 1;");
        write(&root.join("src/b.vue"), "<template />");
        write(&root.join("node_modules/x/index.js"), "");
        write(&root.join("dist/a.js"), "");
        let extensions = vec!["ts".to_string(), "js".to_string()];
        let first = collect_sources(root.to_str().unwrap(), &extensions, &HashMap::new()).unwrap();
        assert_eq!(first.files.len(), 1);
        assert_eq!(first.files[0].path, "src/a.ts");
        assert_eq!(first.files[0].content.as_deref(), Some("export const a = 1;"));

        let known = HashMap::from([("src/a.ts".to_string(), first.files[0].mtime)]);
        let second = collect_sources(root.to_str().unwrap(), &extensions, &known).unwrap();
        assert_eq!(second.files.len(), 1);
        assert!(second.files[0].content.is_none());
    }

    #[test]
    fn sources_respect_nested_gitignores_and_negations_without_a_git_repository() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join(".gitignore"), "generated/\n*.generated.ts\n");
        write(&root.join("src/.gitignore"), "*.ts\n!keep.ts\n");
        write(&root.join("generated/large.ts"), "export const generated = 1;");
        write(&root.join("a.generated.ts"), "export const generated = 2;");
        write(&root.join("src/ignored.ts"), "export const ignored = 1;");
        write(&root.join("src/keep.ts"), "export const keep = 1;");
        let batch = collect_sources(root.to_str().unwrap(), &["ts".into()], &HashMap::new()).unwrap();
        assert_eq!(batch.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["src/keep.ts"]);
        assert!(!batch.truncated);
    }

    #[test]
    fn sources_skip_build_caches_even_without_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for directory in [".codeoss-build", ".pnpm-store", "out-build", "node_modules", "dist", "target"] {
            write(&root.join(directory).join("nested/index.ts"), "export {};");
        }
        write(&root.join("src/index.ts"), "export {};");
        let batch = collect_sources(root.to_str().unwrap(), &["ts".into()], &HashMap::new()).unwrap();
        assert_eq!(batch.files.len(), 1);
        assert_eq!(batch.files[0].path, "src/index.ts");
    }

    #[test]
    fn sources_bound_total_size_including_unchanged_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = "x".repeat(MAX_SOURCE_BYTES as usize);
        let limit = (MAX_SOURCE_TOTAL_BYTES / MAX_SOURCE_BYTES) as usize;
        for i in 0..limit + 2 {
            write(&root.join(format!("{i:03}.ts")), &content);
        }
        write(&root.join("zzz-small.ts"), "export {};");
        let first = collect_sources(root.to_str().unwrap(), &["ts".into()], &HashMap::new()).unwrap();
        assert!(first.truncated);
        assert_eq!(first.files.len(), limit);
        assert_eq!(first.files.iter().map(|file| file.content.as_ref().unwrap().len() as u64).sum::<u64>(), MAX_SOURCE_TOTAL_BYTES);
        let known = first.files.iter().map(|file| (file.path.clone(), file.mtime)).collect();
        let second = collect_sources(root.to_str().unwrap(), &["ts".into()], &known).unwrap();
        assert!(second.truncated);
        assert_eq!(second.files.iter().map(|file| &file.path).collect::<Vec<_>>(), first.files.iter().map(|file| &file.path).collect::<Vec<_>>());
        assert!(second.files.iter().all(|file| file.content.is_none()));
    }

    #[test]
    fn sources_skip_oversized_files_but_keep_small_neighbors() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("large.ts"), &"x".repeat(MAX_SOURCE_BYTES as usize + 1));
        write(&root.join("small.ts"), "export {};");
        let batch = collect_sources(root.to_str().unwrap(), &["ts".into()], &HashMap::new()).unwrap();
        assert!(batch.truncated);
        assert_eq!(batch.files.len(), 1);
        assert_eq!(batch.files[0].path, "small.ts");
    }

    #[test]
    fn limited_reads_reject_growing_or_oversized_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.ts");
        write(&path, "12345");
        assert_eq!(read_limited(&path, 5).as_deref(), Some("12345"));
        assert!(read_limited(&path, 4).is_none());
    }

    #[test]
    fn typings_trim_manifests_and_skip_nested_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("package.json"), r#"{"devDependencies":{"vue":"3"}}"#);
        write(
            &root.join("node_modules/vue/package.json"),
            r#"{"name":"vue","types":"dist/vue.d.ts","scripts":{"x":"y"}}"#,
        );
        write(&root.join("node_modules/vue/dist/vue.d.ts"), "export {};");
        write(&root.join("node_modules/vue/dist/vue.js"), "");
        write(&root.join("node_modules/vue/node_modules/x/index.d.ts"), "export {};");

        let typings = collect_typings(root.to_str().unwrap()).unwrap();
        let paths: Vec<&str> = typings.iter().map(|item| item.path.as_str()).collect();
        assert_eq!(paths, vec!["node_modules/vue/package.json", "node_modules/vue/dist/vue.d.ts"]);
        assert!(!typings[0].content.contains("scripts"));
        assert!(typings[0].content.contains("dist/vue.d.ts"));
    }

    #[test]
    fn typings_resolve_pnpm_siblings_from_real_package_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("package.json"), r#"{"dependencies":{"vue":"3"}}"#);
        let store = root.join("node_modules/.pnpm/vue@3/node_modules");
        write(&store.join("vue/package.json"), r#"{"name":"vue","dependencies":{"@vue/core":"3"}}"#);
        write(&store.join("vue/index.d.ts"), "export {};");
        write(&store.join("@vue/core/package.json"), r#"{"name":"@vue/core"}"#);
        write(&store.join("@vue/core/index.d.ts"), "export {};");
        // 顶层的 vue 在 pnpm 下是符号链接；用 resolve_package 从真实目录出发验证相邻查找。
        let core = resolve_package(&store.join("vue"), "@vue/core").unwrap();
        assert_eq!(core, store.join("@vue/core"));
    }

    #[test]
    fn typings_follow_transitive_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("package.json"), r#"{"dependencies":{"vue":"3"}}"#);
        write(
            &root.join("node_modules/vue/package.json"),
            r#"{"name":"vue","dependencies":{"@vue/core":"3"}}"#,
        );
        write(&root.join("node_modules/vue/index.d.ts"), "export {};");
        write(&root.join("node_modules/@vue/core/package.json"), r#"{"name":"@vue/core"}"#);
        write(&root.join("node_modules/@vue/core/index.d.ts"), "export {};");

        let typings = collect_typings(root.to_str().unwrap()).unwrap();
        let core = typings
            .iter()
            .find(|item| item.path == "node_modules/@vue/core/index.d.ts")
            .unwrap();
        assert_eq!(core.disk_path.as_deref(), Some("node_modules/@vue/core/index.d.ts"));
    }

    #[test]
    fn scoped_packages_map_to_definitely_typed_names() {
        assert_eq!(types_package_name("lodash"), "@types/lodash");
        assert_eq!(types_package_name("@babel/core"), "@types/babel__core");
    }
}
