use std::{
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use crate::models::{
    GitCommitFile, GitFileStatus, GitHistoryItem, GitHistoryPage, GitLineBlame, GitRepositoryStatus,
};

/// 本地 git 命令（status、merge、switch 及其钩子）的上限；网络与提交另有自己的超时。
const LOCAL_TIMEOUT: Duration = Duration::from_secs(120);
/// 进程结束或被杀后，等读线程收尾的最长时间；钩子留下的孙进程可能一直握着管道。
const READER_GRACE: Duration = Duration::from_secs(2);
const MAX_STDERR_BYTES: usize = 1024 * 1024;
const BYTES_PER_MB: usize = 1024 * 1024;
const MAX_SHOW_BYTES: usize = 64 * BYTES_PER_MB;
const MAX_BLAME_BYTES: usize = 32 * BYTES_PER_MB;
const STAGED_DIFF_CHARS: usize = 24_000;
/// UTF-8 每个字符最多 4 字节，读这么多足够截出 `STAGED_DIFF_CHARS` 个字符。
const STAGED_DIFF_READ_BYTES: usize = STAGED_DIFF_CHARS * 4;
const UNLIMITED: usize = usize::MAX;
/// 重命名分支时不会在远程删除这些旧名（远程默认分支另外判断）。
const PROTECTED_BRANCHES: &[&str] = &["main", "master"];
/// 网络操作遇到需要口令或确认主机密钥时直接失败，而不是挂到超时。
const BATCH_SSH_COMMAND: &str = "ssh -o BatchMode=yes";

pub fn status(
    workspace_id: &str,
    name: &str,
    root: &str,
    git_command: &str,
) -> Result<GitRepositoryStatus, String> {
    // 一次 status 同时拿到分支、上游和 ahead/behind，代替原来的 branch / rev-parse / rev-list 三个进程。
    let porcelain = output(
        git_command,
        root,
        &["status", "--porcelain=v1", "-z", "--branch", "--untracked-files=all"],
    )?;
    let (header, entries) = porcelain.split_once('\0').unwrap_or((porcelain.as_str(), ""));
    let tracking = parse_branch_header(header.strip_prefix("## ").unwrap_or_default());
    let changes = parse_status_records(entries);
    let has_remote = remotes(git_command, root).is_ok_and(|items| !items.is_empty());
    let (branches, remote_branches) = ref_names(git_command, root);
    let git_path = output(
        git_command,
        root,
        &["rev-parse", "--git-path", "MERGE_HEAD"],
    )
    .unwrap_or_default();
    let merge_head = PathBuf::from(git_path.trim());
    let merging = if merge_head.is_absolute() {
        merge_head.is_file()
    } else {
        Path::new(root).join(merge_head).is_file()
    };
    Ok(GitRepositoryStatus {
        workspace_id: workspace_id.to_string(),
        name: name.to_string(),
        path: root.to_string(),
        branch: tracking.branch,
        upstream: tracking.upstream,
        ahead: tracking.ahead,
        behind: tracking.behind,
        has_remote,
        merging,
        changes,
        branches,
        remote_branches,
    })
}

struct BranchTracking {
    branch: String,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
}

/// `status --branch` 的头部：`main...origin/main [ahead 1, behind 2]`，
/// 也可能是 `No commits yet on main`、`HEAD (no branch)`、`main...origin/main [gone]`。
fn parse_branch_header(header: &str) -> BranchTracking {
    let header = ["No commits yet on ", "Initial commit on "]
        .iter()
        .find_map(|prefix| header.strip_prefix(prefix))
        .unwrap_or(header);
    let (names, counts) = header.split_once(" [").unwrap_or((header, ""));
    let counts = counts.trim_end_matches(']');
    let (branch, upstream) = names
        .split_once("...")
        .map_or((names, None), |(branch, upstream)| (branch, Some(upstream)));
    let count = |label: &str| {
        counts
            .split(", ")
            .find_map(|part| part.strip_prefix(label)?.parse().ok())
            .unwrap_or(0)
    };
    BranchTracking {
        branch: Some(branch)
            .filter(|branch| *branch != "HEAD (no branch)")
            .unwrap_or_default()
            .to_string(),
        upstream: upstream
            .filter(|_| counts != "gone")
            .map(str::to_string),
        ahead: count("ahead "),
        behind: count("behind "),
    }
}

/// 本地分支与远程分支一次 for-each-ref 取完。
fn ref_names(git_command: &str, root: &str) -> (Vec<String>, Vec<String>) {
    let listing = output(
        git_command,
        root,
        &["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes"],
    )
    .unwrap_or_default();
    let mut branches = Vec::new();
    let mut remote_branches = Vec::new();
    for name in listing.lines().filter(|value| !value.ends_with("/HEAD")) {
        if let Some(branch) = name.strip_prefix("refs/heads/") {
            branches.push(branch.to_string());
        } else if let Some(branch) = name.strip_prefix("refs/remotes/") {
            remote_branches.push(branch.to_string());
        }
    }
    (branches, remote_branches)
}

pub fn stage(root: &str, git_command: &str, paths: &[String], staged: bool) -> Result<(), String> {
    if paths.is_empty() {
        return Err("请选择文件".to_string());
    }
    for path in paths {
        validate_relative(path)?;
    }
    let mut args = if staged {
        vec![LITERAL_PATHSPECS.to_string(), "add".to_string(), "--".to_string()]
    } else {
        vec![
            LITERAL_PATHSPECS.to_string(),
            "restore".to_string(),
            "--staged".to_string(),
            "--".to_string(),
        ]
    };
    args.extend(paths.iter().cloned());
    let result = run(git_command, root, &args);
    if !staged
        && result.is_err()
        && output(git_command, root, &["rev-parse", "--verify", "HEAD"]).is_err()
    {
        let mut fallback = vec![
            LITERAL_PATHSPECS.to_string(),
            "rm".to_string(),
            "--cached".to_string(),
            "--".to_string(),
        ];
        fallback.extend(paths.iter().cloned());
        run(git_command, root, &fallback)
    } else {
        result
    }
}

pub fn discard(root: &str, git_command: &str, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("请选择文件".to_string());
    }
    for path in paths {
        validate_relative(path)?;
    }
    for path in paths {
        let porcelain = status_porcelain(git_command, root, &[path])?;
        let parsed = parse_status_records(&porcelain).into_iter().next();
        let (index_status, worktree_status) = parsed
            .as_ref()
            .map(|item| (item.index_status.as_str(), item.worktree_status.as_str()))
            .unwrap_or(("", ""));
        let tracked = output(
            git_command,
            root,
            &[LITERAL_PATHSPECS, "ls-files", "--error-unmatch", "--", path],
        )
        .is_ok();
        // 未跟踪（含 status 对不上中文/引号路径时）：用 clean 删掉，避免点了撤回却还在。
        if worktree_status == "?"
            || index_status == "?"
            || (parsed.is_none() && !tracked)
        {
            discard_untracked(git_command, root, path)?;
            continue;
        }
        if worktree_status != " " && !worktree_status.is_empty() {
            run_strings(
                git_command,
                root,
                &[LITERAL_PATHSPECS, "restore", "--worktree", "--", path],
            )?;
        }
        if index_status == "A" {
            run_strings(
                git_command,
                root,
                &[LITERAL_PATHSPECS, "restore", "--staged", "--", path],
            )?;
            discard_untracked(git_command, root, path)?;
        } else if index_status != " " && !index_status.is_empty() && worktree_status == " " {
            run_strings(
                git_command,
                root,
                &[LITERAL_PATHSPECS, "restore", "--staged", "--worktree", "--", path],
            )?;
        }
    }
    Ok(())
}

/// 用户给的路径按字面匹配：`*`、`[id].tsx`、`:/` 不能被当成 pathspec 通配或魔法。
/// 只加在接受用户路径的调用上，`stage_all` 里的 `:/` 依赖 pathspec 魔法。
const LITERAL_PATHSPECS: &str = "--literal-pathspecs";

fn discard_untracked(git_command: &str, root: &str, path: &str) -> Result<(), String> {
    let clean_path = path.trim();
    if clean_path.is_empty() || clean_path == "." {
        return Err("不能按仓库根目录撤回未跟踪文件".to_string());
    }
    let is_directory = clean_path.ends_with('/') || Path::new(root).join(clean_path).is_dir();
    let flags = if is_directory { "-fd" } else { "-f" };
    run_strings(git_command, root, &[LITERAL_PATHSPECS, "clean", flags, "--", clean_path])
}

/// `-z` 输出不加引号、不转义，中文和带空格、引号的路径原样返回。
fn status_porcelain(git_command: &str, root: &str, paths: &[&str]) -> Result<String, String> {
    let mut args = vec![
        LITERAL_PATHSPECS,
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
    ];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths);
    }
    output(git_command, root, &args)
}

fn parse_status_records(text: &str) -> Vec<GitFileStatus> {
    let mut records = text.split('\0').filter(|record| !record.is_empty());
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        let Some(change) = parse_status_line(record) else {
            continue;
        };
        // 重命名 / 复制时，原路径作为下一条记录紧跟在后面。
        if [&change.index_status, &change.worktree_status]
            .iter()
            .any(|status| matches!(status.as_str(), "R" | "C"))
        {
            records.next();
        }
        changes.push(change);
    }
    changes
}

fn parse_status_line(line: &str) -> Option<GitFileStatus> {
    let index_status = line.get(0..1)?.to_string();
    let worktree_status = line.get(1..2)?.to_string();
    let path = line.get(3..).filter(|path| !path.is_empty())?;
    Some(GitFileStatus {
        path: path.to_string(),
        staged: index_status != " " && index_status != "?",
        index_status,
        worktree_status,
    })
}

pub fn stage_all(root: &str, git_command: &str, staged: bool) -> Result<(), String> {
    if staged {
        run_strings(git_command, root, &["add", "-A"])
    } else if output(git_command, root, &["rev-parse", "--verify", "HEAD"]).is_err() {
        run_strings(git_command, root, &["rm", "--cached", "-r", "--", "."])
    } else {
        run_strings(git_command, root, &["restore", "--staged", ":/"])
    }
}

pub fn commit(
    root: &str,
    git_command: &str,
    message: &str,
    skip_verify: bool,
) -> Result<(), String> {
    let message = message.trim();
    if message.is_empty() || message.chars().count() > 500 {
        return Err("提交信息不能为空且不能超过 500 个字符".to_string());
    }
    if output(git_command, root, &["diff", "--cached", "--quiet"]).is_ok() {
        return Err("没有已暂存的改动".to_string());
    }
    let mut args = vec!["commit"];
    if skip_verify {
        args.push("--no-verify");
    }
    args.push("-m");
    args.push(message);
    run_with_timeout(git_command, root, &args, Duration::from_secs(120), false)
}

/// 以 `-` 开头的远程名会在 fetch / push 里被当成选项（仓库配置可以写出 `--upload-pack=…`），直接忽略。
pub(crate) fn remotes(git_command: &str, root: &str) -> Result<Vec<String>, String> {
    Ok(output(git_command, root, &["remote"])?
        .lines()
        .map(str::trim)
        .filter(|item| !item.is_empty() && !item.starts_with('-'))
        .map(str::to_string)
        .collect())
}

pub(crate) fn default_remote(git_command: &str, root: &str) -> Result<String, String> {
    let remotes = remotes(git_command, root)?;
    if remotes.iter().any(|item| item == "origin") {
        return Ok("origin".to_string());
    }
    remotes
        .into_iter()
        .next()
        .ok_or_else(|| "没有配置远程仓库".to_string())
}

fn current_upstream(git_command: &str, root: &str) -> Option<String> {
    output(
        git_command,
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

fn ensure_not_merging(root: &str, git_command: &str) -> Result<(), String> {
    let git_path = output(
        git_command,
        root,
        &["rev-parse", "--git-path", "MERGE_HEAD"],
    )?;
    let merge_head = PathBuf::from(git_path.trim());
    let merging = if merge_head.is_absolute() {
        merge_head.is_file()
    } else {
        Path::new(root).join(merge_head).is_file()
    };
    if merging {
        Err("请先解决或中止合并冲突".to_string())
    } else {
        Ok(())
    }
}

pub fn fetch(root: &str, git_command: &str) -> Result<(), String> {
    let remote = default_remote(git_command, root)?;
    run_with_timeout(
        git_command,
        root,
        &["fetch", &remote],
        Duration::from_secs(180),
        true,
    )
}

pub fn pull(root: &str, git_command: &str, skip_verify: bool) -> Result<(), String> {
    ensure_not_merging(root, git_command)?;
    if current_upstream(git_command, root).is_none() {
        return Err("当前分支还没有上游，请先推送并发布分支".to_string());
    }
    let mut args = vec!["pull", "--no-rebase", "--no-edit"];
    if skip_verify {
        args.push("--no-verify");
    }
    run_with_timeout(git_command, root, &args, Duration::from_secs(180), true)
}

pub fn push(root: &str, git_command: &str, skip_verify: bool) -> Result<(), String> {
    ensure_not_merging(root, git_command)?;
    let remote = if current_upstream(git_command, root).is_none() {
        Some(default_remote(git_command, root)?)
    } else {
        None
    };
    let mut args = vec!["push"];
    if skip_verify {
        args.push("--no-verify");
    }
    if let Some(remote) = remote.as_deref() {
        args.extend(["-u", remote, "HEAD"]);
    }
    run_with_timeout(git_command, root, &args, Duration::from_secs(180), true)
}

pub fn sync(root: &str, git_command: &str, skip_verify: bool) -> Result<(), String> {
    if current_upstream(git_command, root).is_none() {
        return push(root, git_command, skip_verify);
    }
    pull(root, git_command, skip_verify)?;
    push(root, git_command, skip_verify)
}

pub fn configured_hooks_path(root: &str, git_command: &str) -> String {
    output(git_command, root, &["config", "--get", "core.hooksPath"])
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn staged_diff(root: &str, git_command: &str) -> Result<String, String> {
    // 生成提交信息时直接把已暂存 diff 交给智能体，不按文件名拦截。
    let (diff, _) = output_limited(
        git_command,
        root,
        &["diff", "--cached", "--unified=1", "--no-ext-diff"],
        STAGED_DIFF_READ_BYTES,
    )?;
    if diff.trim().is_empty() {
        return Err("请先暂存要提交的文件".to_string());
    }
    Ok(diff.chars().take(STAGED_DIFF_CHARS).collect())
}

pub fn switch_branch(
    root: &str,
    git_command: &str,
    branch: &str,
    create: bool,
    from: Option<&str>,
    detach: bool,
) -> Result<(), String> {
    ensure_clean(root, git_command)?;
    validate_branch(git_command, root, branch)?;
    if let Some(start) = from {
        validate_branch(git_command, root, start)?;
    }
    if detach {
        run_strings(git_command, root, &["switch", "--detach", branch])
    } else if create {
        match from {
            Some(start) => run_strings(git_command, root, &["switch", "-c", branch, start]),
            None => run_strings(git_command, root, &["switch", "-c", branch]),
        }
    } else if branch.contains('/')
        && output(
            git_command,
            root,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .is_err()
    {
        run_strings(git_command, root, &["switch", "--track", branch])
    } else {
        run_strings(git_command, root, &["switch", branch])
    }
}

pub fn graph_commit_action(
    root: &str,
    git_command: &str,
    action: &str,
    hash: &str,
    name: &str,
) -> Result<(), String> {
    validate_revision(hash)?;
    match action {
        "detach" => {
            ensure_clean(root, git_command)?;
            run_strings(git_command, root, &["switch", "--detach", hash])
        }
        "cherry-pick" => {
            ensure_clean(root, git_command)?;
            run_strings(git_command, root, &["cherry-pick", hash])
        }
        "branch" => {
            validate_branch(git_command, root, name)?;
            run_strings(git_command, root, &["branch", name, hash])
        }
        "tag" => {
            let tag = name.trim();
            if tag.is_empty() || tag.contains(' ') || tag.starts_with('-') {
                return Err("标记名称无效".to_string());
            }
            run_strings(
                git_command,
                root,
                &["check-ref-format", &format!("refs/tags/{tag}")],
            )?;
            run_strings(git_command, root, &["tag", "--", tag, hash])
        }
        _ => Err("不支持的提交图操作".to_string()),
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRenameReport {
    pub previous_branch: String,
    pub local_renamed: bool,
    pub local_branch: String,
    pub remote_attempted: bool,
    pub remote_ok: bool,
    pub remote_error: String,
    pub had_upstream: bool,
    pub upstream: String,
}

pub fn rename_current_branch(
    root: &str,
    git_command: &str,
    branch: &str,
    rename_remote: bool,
) -> Result<BranchRenameReport, String> {
    let current = output(git_command, root, &["branch", "--show-current"])
        .unwrap_or_default()
        .trim()
        .to_string();
    if current.is_empty() {
        return Err("分离的 HEAD 不能重命名".to_string());
    }
    validate_branch(git_command, root, branch)?;
    let previous_upstream = current_upstream(git_command, root);
    if branch != current {
        run_strings(git_command, root, &["branch", "-m", branch])?;
    }
    let mut remote_attempted = false;
    let mut remote_ok = false;
    let mut remote_error = String::new();
    if let Some(upstream) = previous_upstream.as_deref() {
        if let Some((remote, remote_branch)) = split_upstream(git_command, root, upstream) {
            if remote_branch != branch && rename_remote {
                remote_attempted = true;
                match push_renamed_remote(git_command, root, &remote, branch, remote_branch) {
                    Ok(()) => remote_ok = true,
                    Err(error) => remote_error = error,
                }
            } else if remote_branch != branch {
                let _ = run_strings(git_command, root, &["branch", "--unset-upstream"]);
            }
        }
    }
    let upstream_now = current_upstream(git_command, root).unwrap_or_default();
    Ok(BranchRenameReport {
        previous_branch: current.clone(),
        local_renamed: branch != current,
        local_branch: branch.to_string(),
        remote_attempted,
        remote_ok,
        remote_error,
        had_upstream: previous_upstream.is_some(),
        upstream: upstream_now,
    })
}

/// 按已知远程名前缀拆 `remote/branch`，远程名本身带 `/` 时取最长匹配。
fn split_upstream<'a>(git_command: &str, root: &str, upstream: &'a str) -> Option<(String, &'a str)> {
    remotes(git_command, root)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|remote| {
            let branch = upstream.strip_prefix(remote.as_str())?.strip_prefix('/')?;
            Some((remote, branch))
        })
        .max_by_key(|(remote, _)| remote.len())
}

/// main / master 和远程默认分支（`<remote>/HEAD` 指向的分支）不随重命名删除。
fn is_protected_remote_branch(git_command: &str, root: &str, remote: &str, branch: &str) -> bool {
    PROTECTED_BRANCHES.contains(&branch)
        || output(git_command, root, &["rev-parse", "--abbrev-ref", &format!("{remote}/HEAD")])
            .is_ok_and(|head| head.trim() == format!("{remote}/{branch}"))
}

fn push_renamed_remote(
    git_command: &str,
    root: &str,
    remote: &str,
    branch: &str,
    remote_branch: &str,
) -> Result<(), String> {
    run_with_timeout(
        git_command,
        root,
        &["push", "-u", remote, branch],
        Duration::from_secs(180),
        true,
    )?;
    if is_protected_remote_branch(git_command, root, remote, remote_branch) {
        return Ok(());
    }
    run_with_timeout(
        git_command,
        root,
        &["push", remote, "--delete", remote_branch],
        Duration::from_secs(180),
        true,
    )
}

pub fn set_branch_upstream(
    root: &str,
    git_command: &str,
    upstream: &str,
) -> Result<String, String> {
    let target = upstream.trim();
    if !target.contains('/') || target.starts_with('/') || target.ends_with('/') {
        return Err("请选择远程分支，例如 origin/main".to_string());
    }
    run_strings(git_command, root, &["branch", "--set-upstream-to", target])?;
    current_upstream(git_command, root).ok_or_else(|| "未能读取上游分支".to_string())
}

pub fn merge_branch(root: &str, git_command: &str, branch: &str) -> Result<(), String> {
    ensure_clean(root, git_command)?;
    validate_branch(git_command, root, branch)?;
    run_strings(git_command, root, &["merge", "--no-edit", branch])
}

pub fn abort_merge(root: &str, git_command: &str) -> Result<(), String> {
    run_strings(git_command, root, &["merge", "--abort"])
}

pub fn history(
    root: &str,
    git_command: &str,
    skip: u32,
    limit: u32,
    path: Option<&str>,
) -> Result<GitHistoryPage, String> {
    let limit = limit.clamp(1, 100);
    let take = (limit + 1).to_string();
    let skip = skip.to_string();
    let mut args = vec![
        "log",
        "--date-order",
        "-n",
        take.as_str(),
        "--skip",
        skip.as_str(),
        "--pretty=format:%H%x1f%P%x1f%aN%x1f%aI%x1f%D%x1f%s%x1e",
    ];
    if let Some(path) = path.map(str::trim).filter(|path| !path.is_empty()) {
        args.insert(0, LITERAL_PATHSPECS);
        args.push("--");
        args.push(path);
    }
    let raw = match output(git_command, root, &args) {
        Ok(value) => value,
        Err(message)
            if message.to_ascii_lowercase().contains("does not have any commits")
                || message.to_ascii_lowercase().contains("your current branch") =>
        {
            return Ok(GitHistoryPage {
                items: Vec::new(),
                has_more: false,
            });
        }
        Err(message) => return Err(message),
    };
    let mut items = raw
        .split('\u{1e}')
        .map(str::trim)
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut fields = record.split('\u{1f}');
            let hash = fields.next()?.trim().to_string();
            if hash.is_empty() {
                return None;
            }
            let parents = fields
                .next()
                .unwrap_or("")
                .split_whitespace()
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect();
            let author = fields.next().unwrap_or("").trim().to_string();
            let committed_at = fields.next().unwrap_or("").trim().to_string();
            let refs = fields
                .next()
                .unwrap_or("")
                .split(", ")
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect();
            let subject = fields.next().unwrap_or("").trim().to_string();
            Some(GitHistoryItem {
                hash,
                parents,
                subject,
                author,
                committed_at,
                refs,
            })
        })
        .collect::<Vec<_>>();
    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    Ok(GitHistoryPage { items, has_more })
}

pub fn commit_files(
    root: &str,
    git_command: &str,
    hash: &str,
) -> Result<Vec<GitCommitFile>, String> {
    validate_revision(hash)?;
    let parent = format!("{hash}^");
    let has_parent = output(git_command, root, &["rev-parse", "--verify", "--quiet", &parent]).is_ok();
    let text = if has_parent {
        output(
            git_command,
            root,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-status",
                "-r",
                "-z",
                "--find-renames",
                &parent,
                hash,
            ],
        )?
    } else {
        output(
            git_command,
            root,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-status",
                "-r",
                "-z",
                "--root",
                "--find-renames",
                hash,
            ],
        )?
    };
    // `-z`：状态和路径各占一条记录，重命名 / 复制是“状态、原路径、新路径”三条。
    let mut records = text.split('\0').filter(|record| !record.is_empty());
    let mut files = Vec::new();
    while let Some(code) = records.next() {
        let Some(status) = code.chars().next() else {
            continue;
        };
        if matches!(status, 'R' | 'C') {
            records.next();
        }
        let Some(path) = records.next() else {
            break;
        };
        files.push(GitCommitFile {
            path: path.to_string(),
            status: status.to_string(),
        });
    }
    Ok(files)
}

pub fn revision_file(
    root: &str,
    git_command: &str,
    revision: &str,
    rel_path: &str,
) -> Result<crate::models::GitIndexFile, String> {
    validate_relative(rel_path)?;
    validate_revision(revision)?;
    let inside =
        output(git_command, root, &["rev-parse", "--is-inside-work-tree"]).unwrap_or_default();
    if inside.trim() != "true" {
        return Ok(crate::models::GitIndexFile {
            in_repo: false,
            tracked: false,
            binary: false,
            content: String::new(),
        });
    }
    let spec = format!("{}:{}", revision, rel_path.replace('\\', "/"));
    show_spec(git_command, root, &spec)
}

/// Index (stage 0) contents.
pub fn index_file(
    root: &str,
    git_command: &str,
    rel_path: &str,
) -> Result<crate::models::GitIndexFile, String> {
    validate_relative(rel_path)?;
    let inside =
        output(git_command, root, &["rev-parse", "--is-inside-work-tree"]).unwrap_or_default();
    if inside.trim() != "true" {
        return Ok(crate::models::GitIndexFile {
            in_repo: false,
            tracked: false,
            binary: false,
            content: String::new(),
        });
    }
    let spec = format!(":0:{}", rel_path.replace('\\', "/"));
    show_spec(git_command, root, &spec)
}

pub fn blame_line(
    root: &str,
    git_command: &str,
    rel_path: &str,
    line: u32,
    revision: Option<&str>,
) -> Result<Option<GitLineBlame>, String> {
    validate_relative(rel_path)?;
    if line == 0 {
        return Err("行号无效".to_string());
    }
    if let Some(revision) = revision {
        validate_revision(revision)?;
    }
    let inside =
        output(git_command, root, &["rev-parse", "--is-inside-work-tree"]).unwrap_or_default();
    if inside.trim() != "true" {
        return Ok(None);
    }
    let range = format!("{line},{line}");
    let mut args = vec![LITERAL_PATHSPECS, "blame", "-L", range.as_str(), "--porcelain"];
    if let Some(revision) = revision {
        args.push(revision);
    }
    args.push("--");
    args.push(rel_path);
    match output(git_command, root, &args) {
        Ok(text) => Ok(parse_blame_porcelain(&text)),
        Err(message) if blame_has_no_history(&message) => Ok(Some(uncommitted_blame())),
        Err(message) => Err(message),
    }
}

/// 整文件 blame，一次子进程。编辑器把结果缓存起来，移动光标只查表，不再每换一行起一次 git。
pub fn blame_file(
    root: &str,
    git_command: &str,
    rel_path: &str,
) -> Result<Option<Vec<GitLineBlame>>, String> {
    validate_relative(rel_path)?;
    let inside =
        output(git_command, root, &["rev-parse", "--is-inside-work-tree"]).unwrap_or_default();
    if inside.trim() != "true" {
        return Ok(None);
    }
    match output_limited(
        git_command,
        root,
        &[LITERAL_PATHSPECS, "blame", "--porcelain", "--", rel_path],
        MAX_BLAME_BYTES,
    ) {
        Ok((_, true)) => Err(format!(
            "文件过大（blame 输出超过 {}MB），无法显示逐行作者",
            MAX_BLAME_BYTES / BYTES_PER_MB
        )),
        Ok((text, false)) => Ok(Some(parse_blame_porcelain_file(&text))),
        // 没有历史（新文件）：空列表表示“整个文件都尚未提交”，None 才表示不在仓库里。
        Err(message) if blame_has_no_history(&message) => Ok(Some(Vec::new())),
        Err(message) => Err(message),
    }
}

/// porcelain 里每个提交的作者/摘要只在第一次出现时给出，之后的行只有头部。
pub fn parse_blame_porcelain_file(text: &str) -> Vec<GitLineBlame> {
    let mut commits: std::collections::HashMap<String, GitLineBlame> =
        std::collections::HashMap::new();
    let mut lines = Vec::new();
    let mut current: Option<(String, GitLineBlame)> = None;
    for raw in text.lines() {
        if raw.starts_with('\t') {
            if let Some((hash, info)) = current.take() {
                commits.entry(hash).or_insert_with(|| info.clone());
                lines.push(info);
            }
            continue;
        }
        if let Some((_, info)) = current.as_mut() {
            if let Some(rest) = raw.strip_prefix("author ") {
                info.author_name = rest.to_string();
            } else if let Some(rest) = raw.strip_prefix("author-mail ") {
                info.author_email = rest
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string();
            } else if let Some(rest) = raw.strip_prefix("author-time ") {
                info.author_time = rest.trim().parse().unwrap_or(0);
            } else if let Some(rest) = raw.strip_prefix("summary ") {
                info.subject = rest.to_string();
            }
            continue;
        }
        let mut parts = raw.split_whitespace();
        let Some(hash) = parts.next().filter(|hash| hash.len() >= 8) else {
            continue;
        };
        let committed = hash.chars().any(|char| char != '0');
        let info = commits.get(hash).cloned().unwrap_or(GitLineBlame {
            committed,
            hash: if committed { hash.to_string() } else { String::new() },
            author_name: String::new(),
            author_email: String::new(),
            author_time: 0,
            subject: String::new(),
        });
        current = Some((hash.to_string(), info));
    }
    lines
}

fn uncommitted_blame() -> GitLineBlame {
    GitLineBlame {
        committed: false,
        hash: String::new(),
        author_name: String::new(),
        author_email: String::new(),
        author_time: 0,
        subject: String::new(),
    }
}

fn blame_has_no_history(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("no such path") || missing_from_head(message)
}

pub fn parse_blame_porcelain(text: &str) -> Option<GitLineBlame> {
    let mut lines = text.lines();
    let hash = lines.next()?.split_whitespace().next()?.to_string();
    if hash.len() < 8 {
        return None;
    }
    let mut author_name = String::new();
    let mut author_email = String::new();
    let mut author_time = 0_i64;
    let mut subject = String::new();
    for line in lines {
        if line.starts_with('\t') {
            break;
        } else if let Some(rest) = line.strip_prefix("author ") {
            author_name = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("author-mail ") {
            author_email = rest
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
        } else if let Some(rest) = line.strip_prefix("author-time ") {
            author_time = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("summary ") {
            subject = rest.to_string();
        }
    }
    let committed = hash.chars().any(|char| char != '0');
    Some(GitLineBlame {
        committed,
        hash: if committed { hash } else { String::new() },
        author_name,
        author_email,
        author_time,
        subject,
    })
}

fn show_spec(
    git_command: &str,
    root: &str,
    spec: &str,
) -> Result<crate::models::GitIndexFile, String> {
    match output_limited(git_command, root, &["show", spec], MAX_SHOW_BYTES) {
        Ok((_, true)) => Err(format!(
            "文件超过 {}MB，无法读取该版本内容",
            MAX_SHOW_BYTES / BYTES_PER_MB
        )),
        Ok((content, false)) if content.contains('\0') => Ok(crate::models::GitIndexFile {
            in_repo: true,
            tracked: true,
            binary: true,
            content: String::new(),
        }),
        Ok((content, false)) => Ok(crate::models::GitIndexFile {
            in_repo: true,
            tracked: true,
            binary: false,
            content,
        }),
        Err(message) if message.to_ascii_lowercase().contains("binary") => {
            Ok(crate::models::GitIndexFile {
                in_repo: true,
                tracked: true,
                binary: true,
                content: String::new(),
            })
        }
        Err(message) if missing_from_head(&message) => Ok(crate::models::GitIndexFile {
            in_repo: true,
            tracked: false,
            binary: false,
            content: String::new(),
        }),
        Err(message) => Err(message),
    }
}

fn missing_from_head(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("not in")
        || lower.contains("does not exist")
        || lower.contains("exists on disk")
        || lower.contains("bad revision")
        || lower.contains("needed a single revision")
        || lower.contains("ambiguous argument")
        || lower.contains("not at stage")
        || message.contains("有歧义的参数")
        || message.contains("未知的版本")
        || message.contains("路径不存在")
        || message.contains("不在索引")
}

fn ensure_clean(root: &str, git_command: &str) -> Result<(), String> {
    if output(git_command, root, &["status", "--porcelain"])?
        .trim()
        .is_empty()
    {
        Ok(())
    } else {
        Err("切换或合并分支前请先提交或处理当前改动".to_string())
    }
}

pub(crate) fn validate_branch(git_command: &str, root: &str, branch: &str) -> Result<(), String> {
    if branch.trim() != branch || branch.is_empty() || branch.starts_with('-') {
        return Err("分支名称无效".to_string());
    }
    run_strings(git_command, root, &["check-ref-format", "--branch", branch])
}

fn validate_revision(value: &str) -> Result<(), String> {
    if value == "HEAD" {
        return Ok(());
    }
    let hex = value.strip_suffix('^').unwrap_or(value);
    if (4..=64).contains(&hex.len()) && hex.chars().all(|char| char.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err("修订版本无效".to_string())
}

fn validate_relative(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("文件路径无效".to_string());
    }
    Ok(())
}

/// 所有 git 调用的入口。关掉 fsmonitor：不可信仓库的配置可以借它在轮询 status 时执行任意命令。
pub(crate) fn git_cmd(git_command: &str) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(git_command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.args(["-c", "core.fsmonitor=false"]);
    command
}

pub(crate) struct Captured {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: String,
    /// stdout 超过上限，多出的部分已读掉丢弃。
    pub truncated: bool,
}

/// 带超时执行：超时后杀掉整个进程组（钩子起的孙进程一起），返回 `Ok(None)`；
/// 读线程最多再等 `READER_GRACE`，不会因为孙进程握着管道而无限期挂住。
fn capture(mut command: Command, timeout: Duration, stdout_limit: usize) -> Result<Option<Captured>, String> {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动 Git：{error}"))?;
    let stdout = spawn_reader(child.stdout.take(), stdout_limit);
    let stderr = spawn_reader(child.stderr.take(), MAX_STDERR_BYTES);
    let pid = child.id();
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let _ = done.send(child.wait());
    });
    let Ok(status) = finished.recv_timeout(timeout) else {
        kill_process_tree(pid);
        return Ok(None);
    };
    let status = status.map_err(|error| error.to_string())?;
    let (stdout, truncated) = stdout.recv_timeout(READER_GRACE).unwrap_or_default();
    let (stderr, _) = stderr.recv_timeout(READER_GRACE).unwrap_or_default();
    Ok(Some(Captured {
        status,
        stdout,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        truncated,
    }))
}

fn spawn_reader(stream: Option<impl Read + Send + 'static>, limit: usize) -> mpsc::Receiver<(Vec<u8>, bool)> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut truncated = false;
        if let Some(mut stream) = stream {
            let _ = (&mut stream).take(limit as u64).read_to_end(&mut bytes);
            truncated = io::copy(&mut stream, &mut io::sink()).is_ok_and(|rest| rest > 0);
        }
        let _ = sender.send((bytes, truncated));
    });
    receiver
}

fn kill_process_tree(pid: u32) {
    #[cfg(unix)]
    let _ = crate::process::command("kill")
        .args(["-KILL", &format!("-{pid}")])
        .status();
    #[cfg(windows)]
    let _ = crate::process::command("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

/// 只读查询：`--no-optional-locks` 让后台轮询不去刷新 index，避免和用户自己的 git 命令撞上 index.lock。
pub(crate) fn output(git_command: &str, root: &str, args: &[&str]) -> Result<String, String> {
    output_limited(git_command, root, args, UNLIMITED).map(|(text, _)| text)
}

/// 和 `output` 一样，但 stdout 最多保留 `limit` 字节，第二个值表示是否被截断。
fn output_limited(git_command: &str, root: &str, args: &[&str], limit: usize) -> Result<(String, bool), String> {
    let mut command = git_cmd(git_command);
    command.args(["--no-optional-locks", "-C"]).arg(root).args(args);
    let result = capture(command, LOCAL_TIMEOUT, limit)?.ok_or_else(|| LOCAL_TIMEOUT_MESSAGE.to_string())?;
    if result.status.success() {
        Ok((String::from_utf8_lossy(&result.stdout).into_owned(), result.truncated))
    } else {
        Err(result.stderr.trim().to_string())
    }
}

const LOCAL_TIMEOUT_MESSAGE: &str = "Git 命令超时，可能是钩子或其他进程未结束";

/// 需要自己看退出码的调用（merge-tree、merge、merge-base 等），带本地超时，超时视为出错。
pub(crate) fn exec(git_command: &str, root: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Result<Captured, String> {
    let mut command = git_cmd(git_command);
    command.arg("-C").arg(root).args(args);
    capture(command, LOCAL_TIMEOUT, UNLIMITED)?.ok_or_else(|| LOCAL_TIMEOUT_MESSAGE.to_string())
}

pub(crate) fn run_strings(git_command: &str, root: &str, args: &[&str]) -> Result<(), String> {
    let args = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(git_command, root, &args)
}

fn run(git_command: &str, root: &str, args: &[String]) -> Result<(), String> {
    let mut command = git_cmd(git_command);
    command.arg("-C").arg(root).args(args);
    let result = capture(command, LOCAL_TIMEOUT, 0)?.ok_or_else(|| LOCAL_TIMEOUT_MESSAGE.to_string())?;
    if result.status.success() {
        Ok(())
    } else {
        Err(result.stderr.trim().to_string())
    }
}

pub(crate) fn run_with_timeout(
    git_command: &str,
    root: &str,
    args: &[&str],
    timeout: Duration,
    network: bool,
) -> Result<(), String> {
    let mut command = git_cmd(git_command);
    command.arg("-C").arg(root).args(args);
    if network {
        command.env("GIT_TERMINAL_PROMPT", "0");
        command.env("GCM_INTERACTIVE", "Never");
        if !has_custom_ssh(git_command, root) {
            command.env("GIT_SSH_COMMAND", BATCH_SSH_COMMAND);
        }
    }
    let Some(result) = capture(command, timeout, UNLIMITED)? else {
        return Err(if network {
            "Git 同步超时，请检查网络或远程登录".to_string()
        } else {
            "Git 提交超时，可能是提交钩子未结束".to_string()
        });
    };
    if result.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    let detail = if result.stderr.trim().is_empty() {
        stdout.trim()
    } else {
        result.stderr.trim()
    };
    Err(detail.to_string())
}

/// 用户自己配了 ssh（环境变量或 `core.sshCommand`）时不覆盖。Windows 上 Git 自带 ssh，
/// 换成 PATH 里的 `ssh` 可能连不上原来的 agent，所以只在 Unix 上设置。
fn has_custom_ssh(git_command: &str, root: &str) -> bool {
    cfg!(windows)
        || ["GIT_SSH_COMMAND", "GIT_SSH"]
            .iter()
            .any(|key| std::env::var_os(key).is_some())
        || output(git_command, root, &["config", "--get", "core.sshCommand"]).is_ok()
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::{
        blame_line, commit, commit_files, discard, history, index_file, parse_blame_porcelain,
        graph_commit_action, parse_branch_header, parse_status_records, pull, push, revision_file,
        stage, status, switch_branch,
    };

    #[test]
    fn stages_commits_and_switches_a_repository() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        fs::write(temp.path().join("file.txt"), "first\n").unwrap();

        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        let repository = status("workspace", "repo", root, "git").unwrap();
        assert_eq!(repository.changes.len(), 1);
        assert!(repository.changes[0].staged);
        commit(root, "git", "feat: add file", false).unwrap();

        fs::write(temp.path().join("file.txt"), "second\n").unwrap();
        let repository = status("workspace", "repo", root, "git").unwrap();
        assert!(!repository.changes[0].staged);
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "fix: update file", false).unwrap();
        switch_branch(root, "git", "feature/test", true, None, false).unwrap();
        assert_eq!(
            status("workspace", "repo", root, "git").unwrap().branch,
            "feature/test"
        );
    }

    #[test]
    fn staged_then_modified_is_both_staged_and_unstaged() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        fs::write(temp.path().join("file.txt"), "first\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "feat: add file", false).unwrap();

        fs::write(temp.path().join("file.txt"), "second\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        fs::write(temp.path().join("file.txt"), "third\n").unwrap();

        let change = &status("workspace", "repo", root, "git").unwrap().changes[0];
        assert_eq!(change.path, "file.txt");
        assert_eq!(change.index_status, "M");
        assert_eq!(change.worktree_status, "M");
        assert!(change.staged);

        let index = index_file(root, "git", "file.txt").unwrap();
        assert!(index.in_repo && index.tracked && !index.binary);
        assert_eq!(index.content.replace("\r\n", "\n"), "second\n");
    }

    #[test]
    fn unmerged_index_file_opens_as_missing_instead_of_an_ambiguous_spec() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        fs::write(temp.path().join("file.txt"), "base\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "base", false).unwrap();
        let base = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["branch", "--show-current"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let base = base.trim();
        command(root, &["branch", "other"]);
        fs::write(temp.path().join("file.txt"), "ours\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "ours", false).unwrap();
        command(root, &["switch", "other"]);
        fs::write(temp.path().join("file.txt"), "theirs\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "theirs", false).unwrap();
        let merged = Command::new("git")
            .current_dir(root)
            .args(["merge", "--no-commit", base])
            .output()
            .unwrap();
        assert!(!merged.status.success(), "{}", String::from_utf8_lossy(&merged.stderr));

        let index = index_file(root, "git", "file.txt").unwrap();
        assert!(index.in_repo);
        assert!(!index.tracked);
        assert!(index.content.is_empty());
        let ours = revision_file(root, "git", "HEAD", "file.txt").unwrap();
        assert_eq!(ours.content.replace("\r\n", "\n"), "theirs\n");
    }

    #[test]
    fn parses_nul_separated_status_and_branch_header() {
        let changes = parse_status_records("R  new name.md\0old name.md\0?? 技术需求.md\0 M \"quoted\".txt\0");
        let paths: Vec<_> = changes.iter().map(|change| change.path.as_str()).collect();
        assert_eq!(paths, ["new name.md", "技术需求.md", "\"quoted\".txt"]);
        assert!(changes[0].staged && !changes[1].staged);

        let tracking = parse_branch_header("feature/x...origin/feature/x [ahead 2, behind 3]");
        assert_eq!(tracking.branch, "feature/x");
        assert_eq!(tracking.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!((tracking.ahead, tracking.behind), (2, 3));
        let gone = parse_branch_header("main...origin/main [gone]");
        assert!(gone.upstream.is_none());
        assert_eq!(parse_branch_header("HEAD (no branch)").branch, "");
        assert_eq!(parse_branch_header("No commits yet on main").branch, "main");
    }

    #[test]
    fn literal_pathspecs_do_not_expand_globs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        fs::write(temp.path().join("a.txt"), "a").unwrap();
        fs::write(temp.path().join("b.txt"), "b").unwrap();
        assert!(stage(root, "git", &["*.txt".to_string()], true).is_err());
        let repository = status("workspace", "repo", root, "git").unwrap();
        assert!(repository.changes.iter().all(|change| !change.staged));
    }

    #[test]
    fn rejects_tags_that_look_like_options() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        command(root, &["commit", "--allow-empty", "-m", "first"]);
        assert!(graph_commit_action(root, "git", "tag", "HEAD", "-f").is_err());
        graph_commit_action(root, "git", "tag", "HEAD", "v1").unwrap();
    }

    #[test]
    fn discards_untracked_chinese_markdown() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        command(root, &["config", "core.quotepath", "true"]);
        fs::write(temp.path().join("file.txt"), "first\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "feat: add file", false).unwrap();

        let name = "【技术需求】阿里云OSS.md";
        fs::write(temp.path().join(name), "# oss\n").unwrap();
        let repository = status("workspace", "repo", root, "git").unwrap();
        assert!(repository.changes.iter().any(|change| change.path == name));

        discard(root, "git", &[name.to_string()]).unwrap();
        assert!(!temp.path().join(name).exists());
        assert!(status("workspace", "repo", root, "git")
            .unwrap()
            .changes
            .is_empty());
    }

    #[test]
    fn discards_worktree_edits_and_untracked_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        fs::write(temp.path().join("file.txt"), "first\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "feat: add file", false).unwrap();

        fs::write(temp.path().join("file.txt"), "dirty\n").unwrap();
        fs::write(temp.path().join("extra.txt"), "new\n").unwrap();
        discard(
            root,
            "git",
            &["file.txt".to_string(), "extra.txt".to_string()],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(temp.path().join("file.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "first\n"
        );
        assert!(!temp.path().join("extra.txt").exists());
        assert!(status("workspace", "repo", root, "git")
            .unwrap()
            .changes
            .is_empty());
    }

    #[test]
    fn discarding_one_untracked_file_keeps_other_untracked_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        fs::write(temp.path().join("file.txt"), "first\n").unwrap();
        stage(root, "git", &["file.txt".to_string()], true).unwrap();
        commit(root, "git", "feat: add file", false).unwrap();

        fs::write(temp.path().join("extra.txt"), "new\n").unwrap();
        fs::create_dir(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("nested").join("inside.txt"), "keep\n").unwrap();

        discard(root, "git", &["extra.txt".to_string()]).unwrap();
        assert!(!temp.path().join("extra.txt").exists());
        assert_eq!(
            fs::read_to_string(temp.path().join("nested").join("inside.txt")).unwrap(),
            "keep\n"
        );

        let nested = status("workspace", "repo", root, "git")
            .unwrap()
            .changes
            .into_iter()
            .find(|change| change.path == "nested/inside.txt")
            .expect("untracked file inside a new directory");
        discard(root, "git", &[nested.path]).unwrap();
        assert!(!temp.path().join("nested").join("inside.txt").exists());
        assert!(status("workspace", "repo", root, "git")
            .unwrap()
            .changes
            .is_empty());
    }

    #[test]
    fn publishes_and_pulls_from_a_local_remote() {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        let repo = temp.path().join("repo");
        let other = temp.path().join("other");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&remote).unwrap();
        command(repo.to_str().unwrap(), &["init"]);
        command(
            repo.to_str().unwrap(),
            &["config", "user.email", "test@multara.local"],
        );
        command(
            repo.to_str().unwrap(),
            &["config", "user.name", "Multara Test"],
        );
        command(remote.to_str().unwrap(), &["init", "--bare"]);
        fs::write(repo.join("file.txt"), "one\n").unwrap();
        command(repo.to_str().unwrap(), &["add", "file.txt"]);
        command(repo.to_str().unwrap(), &["commit", "-m", "first"]);
        command(repo.to_str().unwrap(), &["branch", "-M", "main"]);
        command(
            repo.to_str().unwrap(),
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );

        push(repo.to_str().unwrap(), "git", false).unwrap();
        let published = status("workspace", "repo", repo.to_str().unwrap(), "git").unwrap();
        assert!(published.has_remote);
        assert!(published
            .upstream
            .as_deref()
            .is_some_and(|item| item.contains("origin")));
        assert_eq!(published.ahead, 0);

        command(
            temp.path().to_str().unwrap(),
            &[
                "clone",
                "-b",
                "main",
                remote.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        command(
            other.to_str().unwrap(),
            &["config", "user.email", "test@multara.local"],
        );
        command(
            other.to_str().unwrap(),
            &["config", "user.name", "Multara Test"],
        );
        fs::write(other.join("file.txt"), "two\n").unwrap();
        command(other.to_str().unwrap(), &["add", "file.txt"]);
        command(other.to_str().unwrap(), &["commit", "-m", "second"]);
        command(other.to_str().unwrap(), &["push"]);

        pull(repo.to_str().unwrap(), "git", false).unwrap();
        assert_eq!(
            fs::read_to_string(repo.join("file.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "two\n"
        );
    }

    #[test]
    fn logs_history_and_reads_a_commit_file() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        fs::write(temp.path().join("file.txt"), "one\n").unwrap();
        command(root, &["add", "file.txt"]);
        command(root, &["commit", "-m", "first"]);
        fs::write(temp.path().join("file.txt"), "two\n").unwrap();
        command(root, &["add", "file.txt"]);
        command(root, &["commit", "-m", "second"]);

        let page = history(root, "git", 0, 10, None).unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].subject, "second");
        assert_eq!(page.items[0].parents.len(), 1);
        assert!(!page.has_more);
        let files = commit_files(root, "git", &page.items[0].hash).unwrap();
        assert!(files.iter().any(|file| file.path == "file.txt" && file.status == "M"));
        let head = revision_file(root, "git", "HEAD", "file.txt").unwrap();
        assert_eq!(head.content.replace("\r\n", "\n"), "two\n");
        let parent = revision_file(root, "git", &page.items[0].parents[0], "file.txt").unwrap();
        assert_eq!(parent.content.replace("\r\n", "\n"), "one\n");
    }

    #[test]
    fn parses_blame_porcelain_and_reads_a_line() {
        let parsed = parse_blame_porcelain(
            "abcdef1234567890abcdef1234567890abcdef12 1 1 1\n\
             author Ada\n\
             author-mail <ada@example.com>\n\
             author-time 1700000000\n\
             summary Add file\n\
             filename file.txt\n\
             \tline\n",
        )
        .unwrap();
        assert!(parsed.committed);
        assert_eq!(parsed.author_name, "Ada");
        assert_eq!(parsed.author_email, "ada@example.com");
        assert_eq!(parsed.author_time, 1_700_000_000);
        assert_eq!(parsed.subject, "Add file");

        let uncommitted = parse_blame_porcelain(
            "0000000000000000000000000000000000000000 1 1 1\n\
             author Not Committed Yet\n\
             author-mail <not.committed.yet>\n\
             author-time 1700000000\n\
             summary \n\
             \tline\n",
        )
        .unwrap();
        assert!(!uncommitted.committed);

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        command(root, &["init"]);
        command(root, &["config", "user.email", "test@multara.local"]);
        command(root, &["config", "user.name", "Multara Test"]);
        fs::write(temp.path().join("file.txt"), "one\n").unwrap();
        command(root, &["add", "file.txt"]);
        command(root, &["commit", "-m", "first"]);
        let blamed = blame_line(root, "git", "file.txt", 1, None).unwrap().unwrap();
        assert!(blamed.committed);
        assert_eq!(blamed.author_name, "Multara Test");
        assert_eq!(blamed.subject, "first");
        fs::write(temp.path().join("file.txt"), "two\n").unwrap();
        let dirty = blame_line(root, "git", "file.txt", 1, None).unwrap().unwrap();
        assert!(!dirty.committed);
        let head = blame_line(root, "git", "file.txt", 1, Some("HEAD")).unwrap().unwrap();
        assert!(head.committed);
        assert_eq!(head.subject, "first");
    }

    fn command(root: &str, args: &[&str]) {
        assert!(Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
}

#[cfg(test)]
mod blame_file_tests {
    use super::parse_blame_porcelain_file;

    #[test]
    fn reuses_commit_metadata_for_repeated_hashes() {
        let text = concat!(
            "aaaaaaaa1111111111111111111111111111111a 1 1 2\n",
            "author Ann\nauthor-mail <ann@x.dev>\nauthor-time 100\nsummary first\n",
            "filename f.rs\n\tline one\n",
            "aaaaaaaa1111111111111111111111111111111a 2 2\n\tline two\n",
            "0000000000000000000000000000000000000000 3 3 1\n",
            "author Not Committed Yet\nsummary Version of f.rs from f.rs\n\tline three\n",
        );
        let lines = parse_blame_porcelain_file(text);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].author_name, "Ann");
        assert_eq!(lines[1].subject, "first");
        assert!(lines[0].committed && !lines[2].committed);
    }
}
