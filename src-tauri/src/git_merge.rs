//! 把当前分支合并到另一个分支，全程不切换用户正在用的工作区。
//!
//! - 无冲突：用 `git merge-tree --write-tree` 在内存里算出合并结果，再 `commit-tree` 生成合并提交，
//!   不检出任何文件；随后推送到远程和/或更新本地目标分支。
//! - 有冲突：在独立的临时 Worktree 里执行 `git merge --no-commit`，冲突交给用户或 AI 在那里解决，
//!   解决完再提交、推送并删除临时 Worktree。

use std::{fs, path::Path, time::Duration};

use crate::git::{git_cmd, output, remotes, run_strings, run_with_timeout, validate_branch};

/// 合并目标：本地分支名 + 用于推送/对比的远程名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeTarget {
    pub branch: String,
    pub remote: Option<String>,
}

impl MergeTarget {
    pub fn local_ref(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }

    pub fn remote_ref(&self) -> Option<String> {
        self.remote
            .as_ref()
            .map(|remote| format!("refs/remotes/{remote}/{}", self.branch))
    }

    pub fn display(&self) -> String {
        match &self.remote {
            Some(remote) => format!("{remote}/{}", self.branch),
            None => self.branch.clone(),
        }
    }
}

pub enum MergePlan {
    /// 目标分支已经包含当前提交。
    UpToDate,
    /// 可以直接快进到当前提交，不需要合并提交。
    FastForward(String),
    /// 无冲突，已生成合并提交。
    Merged(String),
    /// 有冲突的文件列表。
    Conflict(Vec<String>),
}

#[derive(Default, Debug)]
pub struct ApplyOutcome {
    pub pushed: bool,
    pub local_updated: bool,
    pub notes: Vec<String>,
}

/// 用户在分支列表里选的可能是本地分支 `sit`，也可能是远程分支 `origin/sit`，统一成 (分支名, 远程)。
pub fn resolve_target(git: &str, root: &str, target: &str) -> Result<MergeTarget, String> {
    let target = target.trim();
    let remotes = remotes(git, root).unwrap_or_default();
    for remote in &remotes {
        if let Some(branch) = target.strip_prefix(&format!("{remote}/")) {
            if rev(git, root, &format!("refs/remotes/{target}")).is_some() {
                validate_branch(git, root, branch)?;
                return Ok(MergeTarget {
                    branch: branch.to_string(),
                    remote: Some(remote.clone()),
                });
            }
        }
    }
    validate_branch(git, root, target)?;
    // 本地分支优先用它配置的上游远程，没有时退回 origin / 第一个远程。
    let configured = output(git, root, &["config", "--get", &format!("branch.{target}.remote")])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != ".");
    let remote = configured.or_else(|| {
        remotes
            .iter()
            .find(|item| item.as_str() == "origin")
            .or_else(|| remotes.first())
            .cloned()
    });
    Ok(MergeTarget {
        branch: target.to_string(),
        remote,
    })
}

/// 解析成提交号；引用不存在时返回 None。
pub fn rev(git: &str, root: &str, reference: &str) -> Option<String> {
    output(
        git,
        root,
        &["rev-parse", "--verify", "--quiet", &format!("{reference}^{{commit}}")],
    )
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

pub fn head_commit(git: &str, root: &str) -> Result<String, String> {
    rev(git, root, "HEAD").ok_or_else(|| "当前分支还没有任何提交".to_string())
}

/// 当前分支名；分离 HEAD 时用短提交号。
pub fn head_label(git: &str, root: &str) -> String {
    let branch = output(git, root, &["branch", "--show-current"])
        .unwrap_or_default()
        .trim()
        .to_string();
    if !branch.is_empty() {
        return branch;
    }
    output(git, root, &["rev-parse", "--short", "HEAD"])
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "HEAD".to_string())
}

pub fn is_ancestor(git: &str, root: &str, ancestor: &str, descendant: &str) -> bool {
    git_cmd(git)
        .arg("-C")
        .arg(root)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .status()
        .is_ok_and(|status| status.success())
}

/// 只拉取目标分支，避免大仓库 fetch 全部远程分支太慢。
pub fn fetch_target(git: &str, root: &str, target: &MergeTarget) -> Result<(), String> {
    let Some(remote) = &target.remote else {
        return Ok(());
    };
    let refspec = format!(
        "+refs/heads/{0}:refs/remotes/{1}/{0}",
        target.branch, remote
    );
    run_with_timeout(
        git,
        root,
        &["fetch", "--no-tags", remote, &refspec],
        Duration::from_secs(180),
        true,
    )
}

/// 合并的基准提交：要推送时以远程为准，否则以本地分支为准（本地没有就用远程）。
pub fn target_base(git: &str, root: &str, target: &MergeTarget, push: bool) -> Result<String, String> {
    let remote = target.remote_ref().and_then(|reference| rev(git, root, &reference));
    let local = rev(git, root, &target.local_ref());
    if push {
        if target.remote.is_none() {
            return Err(format!("{} 没有关联远程仓库，无法推送，请取消「推送到远程」", target.branch));
        }
        return remote
            .or(local)
            .ok_or_else(|| format!("找不到目标分支 {}", target.display()));
    }
    local
        .or(remote)
        .ok_or_else(|| format!("找不到目标分支 {}", target.branch))
}

pub fn plan_merge(
    git: &str,
    root: &str,
    base: &str,
    source: &str,
    message: &str,
) -> Result<MergePlan, String> {
    if is_ancestor(git, root, source, base) {
        return Ok(MergePlan::UpToDate);
    }
    if is_ancestor(git, root, base, source) {
        return Ok(MergePlan::FastForward(source.to_string()));
    }
    let result = git_cmd(git)
        .arg("-C")
        .arg(root)
        .args(["merge-tree", "--write-tree", "--name-only", "--no-messages", base, source])
        .output()
        .map_err(|error| format!("无法启动 Git：{error}"))?;
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    let mut lines = stdout.lines().map(str::trim).filter(|line| !line.is_empty());
    match result.status.code() {
        Some(0) => {
            let tree = lines.next().ok_or("Git 没有返回合并结果")?.to_string();
            let commit = output(
                git,
                root,
                &["commit-tree", &tree, "-p", base, "-p", source, "-m", message],
            )?;
            Ok(MergePlan::Merged(commit.trim().to_string()))
        }
        Some(1) => Ok(MergePlan::Conflict(conflict_names(&stdout))),
        _ => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            if stderr.contains("unknown option") || stderr.contains("usage:") {
                return Err("当前 Git 版本过低（需要 2.38 及以上）才能在后台预合并".to_string());
            }
            Err(stderr.trim().to_string())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MergeOutlook {
    /// fast_forward / merge / conflict / up_to_date
    pub kind: &'static str,
    /// 目标分支比当前 HEAD 多出的提交数。
    pub target_ahead: u32,
    pub conflict_files: Vec<String>,
}

/// 预览合并结果，不写提交、不改分支。
pub fn outlook(git: &str, root: &str, base: &str, source: &str) -> Result<MergeOutlook, String> {
    if is_ancestor(git, root, source, base) {
        return Ok(MergeOutlook {
            kind: "up_to_date",
            target_ahead: compare(git, root, source, base).0,
            conflict_files: Vec::new(),
        });
    }
    let target_ahead = compare(git, root, source, base).0;
    if is_ancestor(git, root, base, source) {
        return Ok(MergeOutlook {
            kind: "fast_forward",
            target_ahead,
            conflict_files: Vec::new(),
        });
    }
    let result = git_cmd(git)
        .arg("-C")
        .arg(root)
        .args(["merge-tree", "--write-tree", "--name-only", "--no-messages", base, source])
        .output()
        .map_err(|error| format!("无法启动 Git：{error}"))?;
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    match result.status.code() {
        Some(0) => Ok(MergeOutlook {
            kind: "merge",
            target_ahead,
            conflict_files: Vec::new(),
        }),
        Some(1) => Ok(MergeOutlook {
            kind: "conflict",
            target_ahead,
            conflict_files: conflict_names(&stdout),
        }),
        _ => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            if stderr.contains("unknown option") || stderr.contains("usage:") {
                return Err("当前 Git 版本过低（需要 2.38 及以上）才能在后台预合并".to_string());
            }
            Err(stderr.trim().to_string())
        }
    }
}

fn conflict_names(stdout: &str) -> Vec<String> {
    let mut files = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .skip(1)
        .map(str::to_string)
        .collect::<Vec<_>>();
    files.dedup();
    files
}

/// 把合并结果写到目标分支：按需推送远程，并在安全时更新本地分支。
pub fn apply(
    git: &str,
    root: &str,
    target: &MergeTarget,
    commit: &str,
    push: bool,
) -> Result<ApplyOutcome, String> {
    let mut outcome = ApplyOutcome::default();
    if push {
        let remote = target.remote.as_deref().ok_or("目标分支没有关联远程仓库")?;
        let refspec = format!("{commit}:refs/heads/{}", target.branch);
        run_with_timeout(git, root, &["push", remote, &refspec], Duration::from_secs(180), true)
            .map_err(|detail| explain_push_failure(target, &detail))?;
        outcome.pushed = true;
    }
    match rev(git, root, &target.local_ref()) {
        Some(local) if local == commit => outcome.local_updated = true,
        Some(local) => {
            if !is_ancestor(git, root, &local, commit) {
                outcome.notes.push(format!(
                    "本地 {} 有远程没有的提交，没有自动更新本地分支",
                    target.branch
                ));
            } else if let Some(path) = checked_out_path(git, root, &target.branch) {
                // 目标分支正被某个目录检出时直接改引用会让那个目录看起来全是改动，只在它干净时快进。
                let clean = output(git, &path, &["status", "--porcelain"])
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(false);
                if clean && run_strings(git, &path, &["merge", "--ff-only", commit]).is_ok() {
                    outcome.local_updated = true;
                } else {
                    outcome.notes.push(format!(
                        "本地 {} 正在 {path} 检出且有未提交的改动，请在那里自行拉取",
                        target.branch
                    ));
                }
            } else {
                run_strings(git, root, &["update-ref", &target.local_ref(), commit, &local])?;
                outcome.local_updated = true;
            }
        }
        None if !push => {
            run_strings(git, root, &["branch", &target.branch, commit])?;
            outcome.local_updated = true;
        }
        None => {}
    }
    Ok(outcome)
}

fn explain_push_failure(target: &MergeTarget, detail: &str) -> String {
    let lower = detail.to_lowercase();
    if lower.contains("non-fast-forward") || lower.contains("fetch first") || lower.contains("rejected") {
        return format!("推送被拒绝：{} 有新提交，请先拉取最新再完成合并。", target.display());
    }
    format!("推送到 {} 失败：{}", target.display(), detail.trim())
}

/// 找到检出了该分支的工作目录（主仓库或任意 Worktree）。
pub fn checked_out_path(git: &str, root: &str, branch: &str) -> Option<String> {
    let listing = output(git, root, &["worktree", "list", "--porcelain"]).ok()?;
    let wanted = format!("branch refs/heads/{branch}");
    let mut current: Option<&str> = None;
    for line in listing.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current = Some(path);
        } else if line == wanted {
            return current.map(str::to_string);
        }
    }
    None
}

/// 在临时 Worktree 里开始一次带冲突的合并，返回冲突文件。
pub fn start_conflict_worktree(
    git: &str,
    repository: &str,
    dir: &Path,
    base: &str,
    source: &str,
    message: &str,
) -> Result<Vec<String>, String> {
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let dir_text = dir.to_string_lossy().to_string();
    run_strings(git, repository, &["worktree", "add", "--detach", &dir_text, base])
        .map_err(|detail| format!("创建合并工作区失败：{detail}"))?;
    crate::hook_runtime::link_worktree_runtime(Path::new(repository), dir);
    // 有冲突时 git merge 以非零退出，这是预期的，之后用 MERGE_HEAD 判断是否真的进入了合并状态。
    let merged = git_cmd(git)
        .arg("-C")
        .arg(dir)
        .args(["merge", "--no-ff", "--no-commit", "-m", message, source])
        .output()
        .map_err(|error| format!("无法启动 Git：{error}"));
    let in_merge = rev(git, &dir_text, "MERGE_HEAD").is_some();
    if !in_merge {
        let detail = merged
            .map(|result| String::from_utf8_lossy(&result.stderr).trim().to_string())
            .unwrap_or_else(|error| error);
        remove_worktree(git, repository, &dir_text);
        return Err(format!("在合并工作区里执行合并失败：{detail}"));
    }
    Ok(conflict_files(git, &dir_text))
}

pub fn conflict_files(git: &str, dir: &str) -> Vec<String> {
    output(git, dir, &["diff", "--name-only", "--diff-filter=U"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// 冲突标记必须在行首且正好 7 个字符，避免把 Markdown 分隔线等内容误判成冲突。
pub fn has_conflict_markers(text: &str) -> bool {
    text.lines().any(|line| {
        line.starts_with("<<<<<<< ")
            || line == "<<<<<<<"
            || line.starts_with(">>>>>>> ")
            || line == ">>>>>>>"
    })
}

pub fn files_with_markers(dir: &str, files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| {
            fs::read(Path::new(dir).join(file))
                .map(|bytes| has_conflict_markers(&String::from_utf8_lossy(&bytes)))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

pub enum RemoteUpdate {
    /// 远程目标分支已经包含在当前提交里。
    UpToDate,
    /// 已把远程新提交合进来，工作区是干净的。
    Updated,
    /// 合入远程时出现新的冲突。
    Conflict(Vec<String>),
}

/// 把远程目标分支的新提交合进临时工作区。当前冲突还没提交时先提交，再合并远程。
pub fn incorporate_remote(
    git: &str,
    dir: &str,
    target: &MergeTarget,
    commit_message: &str,
) -> Result<RemoteUpdate, String> {
    let unresolved = files_with_markers(dir, &conflict_files(git, dir));
    if !unresolved.is_empty() {
        return Err(format!("这些文件还有冲突标记：{}", unresolved.join("、")));
    }
    if rev(git, dir, "MERGE_HEAD").is_some() {
        commit_resolution(git, dir, commit_message)?;
    }
    let remote = target
        .remote_ref()
        .ok_or_else(|| format!("{} 没有关联远程仓库", target.branch))?;
    fetch_target(git, dir, target)?;
    if rev(git, dir, &remote).is_none() {
        return Err(format!("找不到远程分支 {}", target.display()));
    }
    if is_ancestor(git, dir, &remote, "HEAD") {
        return Ok(RemoteUpdate::UpToDate);
    }
    let result = git_cmd(git)
        .arg("-C")
        .arg(dir)
        .args(["merge", "--no-edit", &remote])
        .output()
        .map_err(|error| format!("无法启动 Git：{error}"))?;
    if rev(git, dir, "MERGE_HEAD").is_some() {
        return Ok(RemoteUpdate::Conflict(conflict_files(git, dir)));
    }
    if result.status.success() {
        return Ok(RemoteUpdate::Updated);
    }
    Err(String::from_utf8_lossy(&result.stderr).trim().to_string())
}

/// 暂存全部解决结果并提交合并；之前已经提交过（例如上次推送失败）时直接返回当前提交。
pub fn commit_resolution(git: &str, dir: &str, message: &str) -> Result<String, String> {
    if rev(git, dir, "MERGE_HEAD").is_some() {
        let unresolved = files_with_markers(dir, &conflict_files(git, dir));
        if !unresolved.is_empty() {
            return Err(format!("这些文件还有冲突标记：{}", unresolved.join("、")));
        }
        run_strings(git, dir, &["add", "-A"])?;
        run_with_timeout(git, dir, &["commit", "-m", message], Duration::from_secs(120), false)?;
    }
    head_commit(git, dir)
}

pub fn remove_worktree(git: &str, repository: &str, dir: &str) {
    let _ = run_strings(git, repository, &["worktree", "remove", "--force", dir]);
    if Path::new(dir).exists() {
        let _ = fs::remove_dir_all(dir);
    }
    let _ = run_strings(git, repository, &["worktree", "prune"]);
}

/// 「将合入 N 个提交 · M 个文件」：只用本地已有的引用，不联网。
pub fn compare(git: &str, root: &str, target_commit: &str, head: &str) -> (u32, u32) {
    let commits = output(git, root, &["rev-list", "--count", &format!("{target_commit}..{head}")])
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let files = output(git, root, &["diff", "--name-only", &format!("{target_commit}...{head}")])
        .map(|value| value.lines().filter(|line| !line.trim().is_empty()).count() as u32)
        .unwrap_or(0);
    (commits, files)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let result = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(
            result.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8_lossy(&result.stdout).trim().to_string()
    }

    /// 仓库结构：release 上一个提交；feat 和 sit 都从 release 分出。
    fn repository() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        git(dir, &["init", "-q", "-b", "release"]);
        git(dir, &["config", "user.email", "test@example.com"]);
        git(dir, &["config", "user.name", "Test"]);
        fs::write(dir.join("a.txt"), "base\n").unwrap();
        fs::write(dir.join("b.txt"), "base\n").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "base"]);
        git(dir, &["branch", "sit"]);
        git(dir, &["switch", "-q", "-c", "feat"]);
        temp
    }

    fn commit_file(dir: &Path, file: &str, content: &str, message: &str) {
        fs::write(dir.join(file), content).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "-q", "-m", message]);
    }

    #[test]
    fn clean_merge_updates_target_without_touching_current_branch() {
        let temp = repository();
        let dir = temp.path();
        let root = dir.to_str().unwrap();
        commit_file(dir, "a.txt", "feat\n", "feat change");
        git(dir, &["switch", "-q", "sit"]);
        commit_file(dir, "b.txt", "sit\n", "sit change");
        git(dir, &["switch", "-q", "feat"]);

        let target = resolve_target("git", root, "sit").unwrap();
        let base = target_base("git", root, &target, false).unwrap();
        let head = head_commit("git", root).unwrap();
        let MergePlan::Merged(commit) = plan_merge("git", root, &base, &head, "Merge feat into sit").unwrap() else {
            panic!("expected a clean merge");
        };
        let outcome = apply("git", root, &target, &commit, false).unwrap();

        assert!(outcome.local_updated);
        assert_eq!(git(dir, &["branch", "--show-current"]), "feat");
        assert_eq!(git(dir, &["rev-parse", "sit"]), commit);
        assert_eq!(git(dir, &["show", "sit:a.txt"]), "feat");
        assert_eq!(git(dir, &["show", "sit:b.txt"]), "sit");
        assert!(git(dir, &["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn fast_forward_and_up_to_date_are_detected() {
        let temp = repository();
        let dir = temp.path();
        let root = dir.to_str().unwrap();
        let base = rev("git", root, "refs/heads/sit").unwrap();
        assert!(matches!(
            plan_merge("git", root, &base, &head_commit("git", root).unwrap(), "m").unwrap(),
            MergePlan::UpToDate
        ));
        commit_file(dir, "a.txt", "feat\n", "feat change");
        let head = head_commit("git", root).unwrap();
        assert!(matches!(
            plan_merge("git", root, &base, &head, "m").unwrap(),
            MergePlan::FastForward(commit) if commit == head
        ));
        assert_eq!(outlook("git", root, &base, &head).unwrap().kind, "fast_forward");
    }

    #[test]
    fn outlook_marks_a_clean_merge_and_a_conflict() {
        let temp = repository();
        let dir = temp.path();
        let root = dir.to_str().unwrap();
        commit_file(dir, "a.txt", "feat\n", "feat change");
        git(dir, &["switch", "-q", "sit"]);
        commit_file(dir, "b.txt", "sit\n", "sit change");
        git(dir, &["switch", "-q", "feat"]);
        let base = rev("git", root, "refs/heads/sit").unwrap();
        let head = head_commit("git", root).unwrap();
        let clean = outlook("git", root, &base, &head).unwrap();
        assert_eq!(clean.kind, "merge");
        assert!(clean.target_ahead >= 1);

        git(dir, &["switch", "-q", "sit"]);
        commit_file(dir, "a.txt", "sit\n", "sit change");
        git(dir, &["switch", "-q", "feat"]);
        let conflict_base = rev("git", root, "refs/heads/sit").unwrap();
        let conflict = outlook("git", root, &conflict_base, &head_commit("git", root).unwrap()).unwrap();
        assert_eq!(conflict.kind, "conflict");
        assert_eq!(conflict.conflict_files, vec!["a.txt".to_string()]);
    }

    #[test]
    fn conflicts_are_resolved_in_a_separate_worktree() {
        let temp = repository();
        let dir = temp.path();
        let root = dir.to_str().unwrap();
        commit_file(dir, "a.txt", "feat\n", "feat change");
        git(dir, &["switch", "-q", "sit"]);
        commit_file(dir, "a.txt", "sit\n", "sit change");
        git(dir, &["switch", "-q", "feat"]);

        let target = resolve_target("git", root, "sit").unwrap();
        let base = target_base("git", root, &target, false).unwrap();
        let head = head_commit("git", root).unwrap();
        let MergePlan::Conflict(files) = plan_merge("git", root, &base, &head, "m").unwrap() else {
            panic!("expected a conflict");
        };
        assert_eq!(files, vec!["a.txt".to_string()]);

        let merge_dir = temp.path().join(".merge");
        let merge_root = merge_dir.to_str().unwrap();
        let conflicts =
            start_conflict_worktree("git", root, &merge_dir, &base, &head, "Merge feat into sit").unwrap();
        assert_eq!(conflicts, vec!["a.txt".to_string()]);
        assert_eq!(git(dir, &["branch", "--show-current"]), "feat");

        assert!(commit_resolution("git", merge_root, "Merge feat into sit").is_err());
        fs::write(merge_dir.join("a.txt"), "feat + sit\n").unwrap();
        let commit = commit_resolution("git", merge_root, "Merge feat into sit").unwrap();
        apply("git", root, &target, &commit, false).unwrap();
        remove_worktree("git", root, merge_root);

        assert_eq!(git(dir, &["show", "sit:a.txt"]), "feat + sit");
        assert_eq!(git(dir, &["rev-list", "--parents", "-n", "1", "sit"]).split(' ').count(), 3);
        assert!(!merge_dir.exists());
        assert_eq!(git(dir, &["branch", "--show-current"]), "feat");
    }

    #[test]
    fn checked_out_target_with_changes_is_left_alone() {
        let temp = repository();
        let dir = temp.path();
        let root = dir.to_str().unwrap();
        commit_file(dir, "a.txt", "feat\n", "feat change");
        let other = temp.path().join(".sit");
        git(dir, &["worktree", "add", "-q", other.to_str().unwrap(), "sit"]);
        fs::write(other.join("b.txt"), "dirty\n").unwrap();
        let before = git(dir, &["rev-parse", "sit"]);

        let target = resolve_target("git", root, "sit").unwrap();
        let head = head_commit("git", root).unwrap();
        let outcome = apply("git", root, &target, &head, false).unwrap();

        assert!(!outcome.local_updated);
        assert_eq!(outcome.notes.len(), 1);
        assert_eq!(git(dir, &["rev-parse", "sit"]), before);
    }

    #[test]
    fn merges_into_remote_branch_and_pushes() {
        let temp = repository();
        let dir = temp.path();
        let root = dir.to_str().unwrap();
        let remote = temp.path().join(".remote.git");
        git(dir, &["init", "-q", "--bare", remote.to_str().unwrap()]);
        git(dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(dir, &["push", "-q", "origin", "release", "sit"]);
        git(dir, &["branch", "-D", "sit"]);
        commit_file(dir, "a.txt", "feat\n", "feat change");

        let target = resolve_target("git", root, "origin/sit").unwrap();
        assert_eq!(target, MergeTarget { branch: "sit".into(), remote: Some("origin".into()) });
        fetch_target("git", root, &target).unwrap();
        let base = target_base("git", root, &target, true).unwrap();
        let head = head_commit("git", root).unwrap();
        let MergePlan::FastForward(commit) = plan_merge("git", root, &base, &head, "m").unwrap() else {
            panic!("expected a fast-forward");
        };
        let outcome = apply("git", root, &target, &commit, true).unwrap();

        assert!(outcome.pushed);
        assert!(!outcome.local_updated);
        let pushed = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["rev-parse", "sit"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&pushed.stdout).trim(), head);
        assert_eq!(git(dir, &["branch", "--show-current"]), "feat");
    }

    #[test]
    fn conflict_markers_need_to_start_the_line() {
        assert!(has_conflict_markers("a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> feat\n"));
        assert!(!has_conflict_markers("text with <<<<<<< inside\n=======\n"));
    }
}
