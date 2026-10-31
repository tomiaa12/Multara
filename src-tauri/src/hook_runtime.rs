use std::{
    fs,
    io::{self, Read},
    path::Path,
};

pub const DIAGNOSTIC_PREFIX: &str = "@@multara-hook@@";

const MANIFESTS: &[&str] = &[
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "npm-shrinkwrap.json",
];

#[derive(Clone, Copy)]
pub enum GitOperation {
    Push,
    Commit,
    Pull,
    Sync,
}

#[derive(Default)]
pub struct RuntimeLinkReport {
    pub linked_husky: bool,
    pub linked_node_modules: bool,
    pub skipped_node_modules: bool,
    pub husky_error: Option<String>,
    pub node_modules_error: Option<String>,
}

enum Failure {
    LocalHook { hook: String, offending: String },
    RemoteRejected,
    AuthOrNetwork,
    Unknown,
}

pub fn link_worktree_runtime(source: &Path, worktree: &Path) -> RuntimeLinkReport {
    let mut report = RuntimeLinkReport::default();
    if source == worktree {
        return report;
    }
    let source_husky = source.join(".husky/_/husky.sh");
    let worktree_husky = worktree.join(".husky/_/husky.sh");
    if source_husky.is_file() {
        match ensure_symlink(&source_husky, &worktree_husky, false) {
            LinkResult::Linked => report.linked_husky = true,
            LinkResult::Already => {}
            LinkResult::Failed(error) => report.husky_error = Some(error),
        }
        // husky 安装时会写 `.husky/_/.gitignore`（内容 `*`），它本身被忽略、不会检出到 worktree。
        let ignore = worktree.join(".husky/_/.gitignore");
        if worktree_husky.symlink_metadata().is_ok() && ignore.symlink_metadata().is_err() {
            let _ = fs::write(&ignore, "*\n");
        }
    }
    let source_modules = source.join("node_modules");
    let worktree_modules = worktree.join("node_modules");
    if worktree.join("package.json").is_file()
        && source_modules.exists()
        && !worktree_modules.exists()
    {
        if dependency_manifests_match(source, worktree) {
            match ensure_symlink(&source_modules, &worktree_modules, true) {
                LinkResult::Linked => report.linked_node_modules = true,
                LinkResult::Already => {}
                LinkResult::Failed(error) => report.node_modules_error = Some(error),
            }
        } else {
            report.skipped_node_modules = true;
        }
    }
    report
}

pub fn describe_repair(report: &RuntimeLinkReport) -> String {
    let mut parts = Vec::new();
    if report.linked_husky {
        parts.push(tr!("已将 .husky/_/husky.sh 链接到主检出。"));
    }
    if report.linked_node_modules {
        parts.push(tr!("已将 node_modules 链接到主检出。"));
    }
    if report.skipped_node_modules {
        parts.push(tr!("依赖与主检出不同，未链接 node_modules。"));
    }
    if let Some(error) = &report.husky_error {
        parts.push(tr!("未能链接 .husky/_/husky.sh：{error}。", error = error));
    }
    if let Some(error) = &report.node_modules_error {
        parts.push(tr!("未能链接 node_modules：{error}。", error = error));
    }
    if parts.is_empty() {
        return tr!("没有需要补上的钩子运行时。");
    }
    parts.concat()
}

pub fn preflight(worktree: &str, source: &str, operation: GitOperation) -> Result<(), String> {
    let worktree_path = Path::new(worktree);
    let source_path = Path::new(source);
    if worktree_path == source_path {
        return Ok(());
    }
    let missing_husky = source_path.join(".husky/_/husky.sh").is_file()
        && !worktree_path.join(".husky/_/husky.sh").exists();
    let missing_modules = worktree_needs_linked_modules(source_path, worktree_path);
    let manifests_match = dependency_manifests_match(source_path, worktree_path);
    if !missing_husky && !missing_modules {
        return Ok(());
    }
    let mut lines = vec![blocked_by_hooks(operation)];
    if missing_husky {
        lines.push(tr!("缺少 .husky/_/husky.sh"));
    }
    if missing_modules && manifests_match {
        lines.push(tr!("缺少 node_modules，可链接主检出"));
    }
    if missing_modules && !manifests_match {
        lines.push(tr!("依赖与主检出不同，请单独安装"));
    }
    let sentences = lines.join("\n");
    let kind = if missing_modules && !manifests_match && !missing_husky {
        "deps_mismatch"
    } else {
        "local_hook"
    };
    let repairable = missing_husky || (missing_modules && manifests_match);
    let skippable = true;
    Err(diagnostic(
        kind, repairable, skippable, operation, &sentences,
    ))
}

pub fn explain_failure(operation: GitOperation, detail: &str, hooks_path: &str) -> String {
    if detail.contains(DIAGNOSTIC_PREFIX) {
        return detail.to_string();
    }
    match classify_failure(detail, hooks_path) {
        Failure::AuthOrNetwork => tr!("远程认证或网络失败：{detail}", detail = detail),
        Failure::RemoteRejected => remote_rejected(operation, detail),
        Failure::LocalHook { hook, offending } => {
            let (message, repairable) = local_hook_message(operation, &hook, &offending);
            let skippable = hook != "post-merge";
            diagnostic("local_hook", repairable, skippable, operation, &message)
        }
        Failure::Unknown => detail.to_string(),
    }
}

fn worktree_needs_linked_modules(source: &Path, worktree: &Path) -> bool {
    worktree.join("package.json").is_file()
        && source.join("node_modules").exists()
        && !worktree.join("node_modules").exists()
        && hooks_need_node(worktree)
}

fn hooks_need_node(worktree: &Path) -> bool {
    let Ok(entries) = fs::read_dir(worktree.join(".husky")) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("_") || !path.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text.contains("node_modules") || text.contains("node ") || text.contains("node\t") {
            return true;
        }
    }
    false
}

fn dependency_manifests_match(source: &Path, worktree: &Path) -> bool {
    let source_package = source.join("package.json");
    let worktree_package = worktree.join("package.json");
    if !source_package.is_file()
        || !worktree_package.is_file()
        || !files_equal(&source_package, &worktree_package)
    {
        return false;
    }
    for name in MANIFESTS {
        let left = source.join(name);
        let right = worktree.join(name);
        match (left.is_file(), right.is_file()) {
            (false, false) => {}
            (true, true) if files_equal(&left, &right) => {}
            _ => return false,
        }
    }
    true
}

fn files_equal(left: &Path, right: &Path) -> bool {
    let Ok(left_file) = fs::File::open(left) else {
        return false;
    };
    let Ok(right_file) = fs::File::open(right) else {
        return false;
    };
    if left_file.metadata().ok().map(|meta| meta.len())
        != right_file.metadata().ok().map(|meta| meta.len())
    {
        return false;
    }
    let mut left_file = io::BufReader::new(left_file);
    let mut right_file = io::BufReader::new(right_file);
    let mut left_buf = [0_u8; 8192];
    let mut right_buf = [0_u8; 8192];
    loop {
        let left_read = read_exact_chunk(&mut left_file, &mut left_buf);
        let right_read = read_exact_chunk(&mut right_file, &mut right_buf);
        if left_read != right_read || left_buf[..left_read] != right_buf[..right_read] {
            return false;
        }
        if left_read == 0 {
            return true;
        }
    }
}

fn read_exact_chunk(reader: &mut impl Read, buf: &mut [u8]) -> usize {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return filled,
        }
    }
    filled
}

enum LinkResult {
    Linked,
    Already,
    Failed(String),
}

fn ensure_symlink(source: &Path, dest: &Path, directory: bool) -> LinkResult {
    if dest.exists() {
        return LinkResult::Already;
    }
    if dest.symlink_metadata().is_ok() && fs::remove_file(dest).is_err() {
        return LinkResult::Failed(tr!("无法替换失效的符号链接"));
    }
    if let Some(parent) = dest.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return LinkResult::Failed(error.to_string());
        }
    }
    match symlink_to(source, dest, directory) {
        Ok(()) => LinkResult::Linked,
        Err(error) => LinkResult::Failed(error.to_string()),
    }
}

fn symlink_to(source: &Path, dest: &Path, directory: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = directory;
        std::os::unix::fs::symlink(source, dest)
    }
    #[cfg(windows)]
    {
        if directory {
            std::os::windows::fs::symlink_dir(source, dest)
        } else {
            std::os::windows::fs::symlink_file(source, dest)
        }
    }
}

fn classify_failure(detail: &str, hooks_path: &str) -> Failure {
    let lines: Vec<&str> = detail.lines().collect();
    if lines.iter().any(|line| {
        line.contains("Authentication failed")
            || line.contains("could not read Username")
            || line.contains("Could not resolve host")
            || line.contains("terminal prompts disabled")
    }) {
        return Failure::AuthOrNetwork;
    }
    if lines.iter().any(|line| {
        line.starts_with("remote:")
            || line.contains("[rejected]")
            || line.contains("non-fast-forward")
            || line.contains("Updates were rejected")
    }) {
        return Failure::RemoteRejected;
    }
    if let Some(offending) = lines.iter().find(|line| hook_line(line, hooks_path)) {
        return Failure::LocalHook {
            hook: extract_hook_name(offending),
            offending: offending.trim().to_string(),
        };
    }
    Failure::Unknown
}

fn hook_line(line: &str, hooks_path: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || line.starts_with("remote:") || line.contains("failed to push some refs") {
        return false;
    }
    if line.contains("pre-push")
        || line.contains("commit-msg")
        || line.contains("post-merge")
        || line.contains("pre-commit")
        || line.contains("pre-merge-commit")
    {
        return true;
    }
    let hooks_path = hooks_path.trim();
    hooks_path.len() >= 2 && line.contains(hooks_path)
}

fn extract_hook_name(line: &str) -> String {
    for name in [
        "pre-merge-commit",
        "pre-push",
        "commit-msg",
        "post-merge",
        "pre-commit",
    ] {
        if line.contains(name) {
            return name.to_string();
        }
    }
    "hook".to_string()
}

fn local_hook_message(operation: GitOperation, hook: &str, offending: &str) -> (String, bool) {
    let mut lines = vec![blocked_by_hook(operation, hook)];
    let mut repairable = false;
    if offending.contains("husky.sh") || offending.contains(".husky/_") {
        lines.push(tr!("缺少 .husky/_/husky.sh"));
        repairable = true;
    } else if offending.contains("node_modules") || offending.contains("Cannot find module") {
        lines.push(tr!("缺少 node_modules"));
        repairable = true;
    } else {
        lines.push(offending.to_string());
    }
    (lines.join("\n"), repairable)
}

fn diagnostic(
    kind: &str,
    repairable: bool,
    skippable: bool,
    operation: GitOperation,
    message: &str,
) -> String {
    let payload = serde_json::json!({
        "kind": kind,
        "repairable": repairable,
        "skippable": skippable,
        "operation": operation_name(operation),
    });
    format!("{DIAGNOSTIC_PREFIX}{payload}\n{message}")
}

/// 动词在各语言里会改变语序和助词，按操作写成整句模板。
fn blocked_by_hooks(operation: GitOperation) -> String {
    match operation {
        GitOperation::Push => tr!("推送被本地钩子拦住"),
        GitOperation::Commit => tr!("提交被本地钩子拦住"),
        GitOperation::Pull => tr!("拉取被本地钩子拦住"),
        GitOperation::Sync => tr!("同步被本地钩子拦住"),
    }
}

fn blocked_by_hook(operation: GitOperation, hook: &str) -> String {
    match operation {
        GitOperation::Push => tr!("推送被本地 {hook} 拦住", hook = hook),
        GitOperation::Commit => tr!("提交被本地 {hook} 拦住", hook = hook),
        GitOperation::Pull => tr!("拉取被本地 {hook} 拦住", hook = hook),
        GitOperation::Sync => tr!("同步被本地 {hook} 拦住", hook = hook),
    }
}

fn remote_rejected(operation: GitOperation, detail: &str) -> String {
    match operation {
        GitOperation::Push => tr!("远程拒绝了这次推送：{detail}", detail = detail),
        GitOperation::Commit => tr!("远程拒绝了这次提交：{detail}", detail = detail),
        GitOperation::Pull => tr!("远程拒绝了这次拉取：{detail}", detail = detail),
        GitOperation::Sync => tr!("远程拒绝了这次同步：{detail}", detail = detail),
    }
}

fn operation_name(operation: GitOperation) -> &'static str {
    match operation {
        GitOperation::Push => "push",
        GitOperation::Commit => "commit",
        GitOperation::Pull => "pull",
        GitOperation::Sync => "sync",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUSKY_STDERR: &str = "\
.husky/pre-push: line 2: .husky/_/husky.sh: No such file or directory
error: failed to push some refs to 'http://example.test/pages.git'";

    #[test]
    fn classifies_a_missing_husky_shim_as_a_local_hook() {
        let message = explain_failure(GitOperation::Push, HUSKY_STDERR, ".husky");
        assert!(message.contains(DIAGNOSTIC_PREFIX));
        assert!(message.contains("推送被本地 pre-push 拦住"));
        assert!(message.contains("缺少 .husky/_/husky.sh"));
        assert!(message.contains("\"repairable\":true"));
        assert!(message.contains("\"operation\":\"push\""));
    }

    #[test]
    fn does_not_treat_an_empty_hooks_path_as_a_hook_match() {
        let detail = "error: something else failed";
        assert_eq!(explain_failure(GitOperation::Push, detail, ""), detail);
    }

    #[test]
    fn classifies_remote_rejection_and_auth_before_hooks() {
        let rejected = explain_failure(
            GitOperation::Push,
            "remote: protected branch\n! [rejected] main -> main (non-fast-forward)",
            ".husky",
        );
        assert!(rejected.starts_with("远程拒绝了这次推送"));
        assert!(!rejected.contains(DIAGNOSTIC_PREFIX));

        let auth = explain_failure(
            GitOperation::Push,
            "fatal: could not read Username for 'http://example.test': terminal prompts disabled",
            ".husky",
        );
        assert!(auth.starts_with("远程认证或网络失败"));
    }

    #[test]
    fn links_husky_and_matching_node_modules() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let worktree = temp.path().join("worktree");
        fs::create_dir_all(source.join(".husky/_")).unwrap();
        fs::create_dir_all(source.join("node_modules/@fs")).unwrap();
        fs::write(source.join(".husky/_/husky.sh"), "echo husky\n").unwrap();
        fs::write(source.join("package.json"), "{\"name\":\"app\"}\n").unwrap();
        fs::write(source.join("package-lock.json"), "{}\n").unwrap();
        fs::write(
            source.join("node_modules/@fs/check.js"),
            "module.exports = 1\n",
        )
        .unwrap();
        fs::create_dir_all(worktree.join(".husky")).unwrap();
        fs::write(worktree.join("package.json"), "{\"name\":\"app\"}\n").unwrap();
        fs::write(worktree.join("package-lock.json"), "{}\n").unwrap();
        fs::write(
            worktree.join(".husky/pre-push"),
            "#!/bin/sh\nnode ./node_modules/@fs/check.js\n",
        )
        .unwrap();

        let report = link_worktree_runtime(&source, &worktree);
        assert!(report.linked_husky);
        assert!(report.linked_node_modules);
        assert_eq!(
            fs::read_to_string(worktree.join(".husky/_/husky.sh")).unwrap(),
            "echo husky\n"
        );
        assert_eq!(
            fs::read_to_string(worktree.join(".husky/_/.gitignore")).unwrap(),
            "*\n"
        );
        assert!(preflight(
            worktree.to_str().unwrap(),
            source.to_str().unwrap(),
            GitOperation::Push
        )
        .is_ok());
    }

    #[test]
    fn does_not_link_node_modules_when_the_lockfile_differs() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let worktree = temp.path().join("worktree");
        fs::create_dir_all(source.join("node_modules")).unwrap();
        fs::write(source.join("package.json"), "{\"name\":\"app\"}\n").unwrap();
        fs::write(source.join("pnpm-lock.yaml"), "lock: source\n").unwrap();
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join("package.json"), "{\"name\":\"app\"}\n").unwrap();
        fs::write(worktree.join("pnpm-lock.yaml"), "lock: branch\n").unwrap();
        fs::create_dir_all(worktree.join(".husky")).unwrap();
        fs::write(
            worktree.join(".husky/pre-push"),
            "node ./node_modules/x.js\n",
        )
        .unwrap();

        let report = link_worktree_runtime(&source, &worktree);
        assert!(report.skipped_node_modules);
        assert!(!worktree.join("node_modules").exists());
        let error = preflight(
            worktree.to_str().unwrap(),
            source.to_str().unwrap(),
            GitOperation::Push,
        )
        .unwrap_err();
        assert!(error.contains("依赖与主检出不同"));
        assert!(error.contains("\"kind\":\"deps_mismatch\""));
        assert!(error.contains("\"skippable\":true"));
    }

    #[test]
    fn preflight_on_sync_can_still_be_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let worktree = temp.path().join("worktree");
        fs::create_dir_all(source.join(".husky/_")).unwrap();
        fs::write(source.join(".husky/_/husky.sh"), "echo husky\n").unwrap();
        fs::create_dir_all(&worktree).unwrap();
        let error = preflight(
            worktree.to_str().unwrap(),
            source.to_str().unwrap(),
            GitOperation::Sync,
        )
        .unwrap_err();
        assert!(error.contains("\"skippable\":true"));
        assert!(error.contains(".husky/_/husky.sh"));
    }
}
