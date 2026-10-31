use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::models::{Workspace, WorkspaceCandidate};

pub struct PreparedWorkspace {
    pub workspace: Workspace,
    pub branch: Option<String>,
}

pub fn prepare(
    app: &AppHandle,
    project_id: &str,
    source_path: &str,
    mode: &str,
    isolation_base: Option<PathBuf>,
    git_command: &str,
    base_ref: Option<&str>,
    checkout_name: Option<&str>,
    create_branch: bool,
) -> Result<PreparedWorkspace, String> {
    let requested = Path::new(source_path)
        .canonicalize()
        .map_err(|error| tr!("无法打开目录：{error}", error = error))?;
    if !requested.is_dir() {
        return Err(tr!("只能添加文件夹"));
    }

    let workspace_id = Uuid::new_v4().to_string();
    let git_root = find_git_root(&requested, git_command);
    let source = git_root.as_deref().unwrap_or(&requested);
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("workspace")
        .to_string();
    let kind = if git_root.is_some() { "git" } else { "folder" };
    let created_at = chrono::Utc::now().to_rfc3339();

    if mode == "direct" {
        return Ok(PreparedWorkspace {
            workspace: Workspace {
                id: workspace_id,
                project_id: project_id.to_string(),
                session_id: None,
                source_workspace_id: None,
                name,
                kind,
                source_path: source.to_string_lossy().to_string(),
                work_path: source.to_string_lossy().to_string(),
                branch: current_branch(source, git_command),
                isolation_mode: "direct",
                created_at,
            },
            branch: None,
        });
    }

    let isolation_root = isolation_base
        .map(Ok)
        .unwrap_or_else(|| app.path().app_data_dir().map_err(|error| error.to_string()))?
        .join("workspaces")
        .join(project_id)
        .join(&workspace_id);
    let result = if kind == "git" {
        let branch = checkout_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| name.to_string())
            .unwrap_or_else(|| {
                format!(
                    "multara/{}/{}",
                    short_id(project_id),
                    short_id(&workspace_id)
                )
            });
        create_worktree(
            source,
            &isolation_root,
            &branch,
            base_ref.unwrap_or("HEAD"),
            create_branch,
            git_command,
        )?;
        PreparedWorkspace {
            workspace: Workspace {
                id: workspace_id,
                project_id: project_id.to_string(),
                session_id: None,
                source_workspace_id: None,
                name,
                kind,
                source_path: source.to_string_lossy().to_string(),
                work_path: isolation_root.to_string_lossy().to_string(),
                branch: Some(branch.clone()),
                isolation_mode: "worktree",
                created_at,
            },
            branch: Some(branch),
        }
    } else {
        if let Err(error) = copy_directory(source, &isolation_root) {
            let _ = fs::remove_dir_all(&isolation_root);
            return Err(error);
        }
        PreparedWorkspace {
            workspace: Workspace {
                id: workspace_id,
                project_id: project_id.to_string(),
                session_id: None,
                source_workspace_id: None,
                name,
                kind,
                source_path: source.to_string_lossy().to_string(),
                work_path: isolation_root.to_string_lossy().to_string(),
                branch: None,
                isolation_mode: "copy",
                created_at,
            },
            branch: None,
        }
    };
    Ok(result)
}

const MAX_DISCOVER_DIRECTORIES: usize = 100;

pub fn discover(source_path: &str, git_command: &str) -> Result<Vec<WorkspaceCandidate>, String> {
    let root = Path::new(source_path)
        .canonicalize()
        .map_err(|error| tr!("无法打开目录：{error}", error = error))?;
    if !root.is_dir() {
        return Err(tr!("只能扫描文件夹"));
    }
    if find_git_root(&root, git_command).as_deref() == Some(root.as_path()) {
        return Ok(vec![candidate(&root, true, git_command)]);
    }
    let mut directories = fs::read_dir(&root)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| {
            let visible = !entry.file_name().to_string_lossy().starts_with('.');
            visible
                && entry
                    .file_type()
                    .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    // 每个子目录要起 2~3 个 git 进程，选到 ~/code 这类大目录时只看按名字排序的前若干个。
    directories.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    });
    directories.truncate(MAX_DISCOVER_DIRECTORIES);
    let mut candidates = directories
        .into_iter()
        .filter_map(|path| {
            let path = path.canonicalize().ok()?;
            let is_git = find_git_root(&path, git_command).as_deref() == Some(path.as_path());
            Some(candidate(&path, is_git, git_command))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.name.to_lowercase());
    if candidates.is_empty() {
        candidates.push(candidate(&root, false, git_command));
    }
    Ok(candidates)
}

fn candidate(path: &Path, is_git: bool, git_command: &str) -> WorkspaceCandidate {
    WorkspaceCandidate {
        path: path.to_string_lossy().to_string(),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace")
            .to_string(),
        kind: if is_git { "git" } else { "folder" },
        branches: if is_git {
            git_branches(path, git_command)
        } else {
            Vec::new()
        },
        current_branch: is_git.then(|| current_branch(path, git_command)).flatten(),
    }
}

fn git_branches(path: &Path, git_command: &str) -> Vec<String> {
    let Ok(output) = git_cmd(git_command)
        .arg("-C")
        .arg(path)
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let mut branches = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|branch| !branch.is_empty() && !branch.ends_with("/HEAD"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    branches.sort();
    branches.dedup();
    branches
}

pub fn cleanup(prepared: &PreparedWorkspace, git_command: &str) {
    let workspace = &prepared.workspace;
    if workspace.isolation_mode == "worktree" {
        let _ = git_cmd(git_command)
            .args([
                "-C",
                &workspace.source_path,
                "worktree",
                "remove",
                "--force",
                &workspace.work_path,
            ])
            .output();
        if let Some(branch) = &prepared.branch {
            let _ = git_cmd(git_command)
                .args(["-C", &workspace.source_path, "branch", "-D", branch])
                .output();
        }
    } else if workspace.isolation_mode == "copy" {
        let _ = fs::remove_dir_all(&workspace.work_path);
    }
}

pub fn remove(workspace: &Workspace, git_command: &str) {
    cleanup(
        &PreparedWorkspace {
            workspace: workspace.clone(),
            branch: workspace.branch.clone(),
        },
        git_command,
    );
}

/// 从需求里卸掉 Worktree 时只删独立检出目录，不删分支，也不动 source_path。
/// 目录已经不在时视为成功，并 prune 掉 git 里的失效记录。
pub fn drop_worktree_checkout(workspace: &Workspace, git_command: &str) -> Result<(), String> {
    if workspace.isolation_mode != "worktree" {
        return Ok(());
    }
    let output = git_cmd(git_command)
        .args([
            "-C",
            &workspace.source_path,
            "worktree",
            "remove",
            "--force",
            &workspace.work_path,
        ])
        .output()
        .map_err(|error| tr!("无法删除 Worktree：{error}", error = error))?;
    if output.status.success() || !Path::new(&workspace.work_path).exists() {
        let _ = git_cmd(git_command)
            .args(["-C", &workspace.source_path, "worktree", "prune"])
            .output();
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    Err(tr!(
        "Worktree 目录还在，没有从需求里移除：{detail}",
        detail = detail.trim()
    ))
}

fn find_git_root(path: &Path, git_command: &str) -> Option<PathBuf> {
    let output = git_cmd(git_command)
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8(output.stdout).ok()?;
    Path::new(root.trim()).canonicalize().ok()
}

fn current_branch(path: &Path, git_command: &str) -> Option<String> {
    let output = git_cmd(git_command)
        .arg("-C")
        .arg(path)
        .args(["branch", "--show-current"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty())
}

fn git_cmd(git_command: &str) -> Command {
    crate::git::git_cmd(git_command)
}

fn create_worktree(
    source: &Path,
    target: &Path,
    branch: &str,
    base_ref: &str,
    create_branch: bool,
    git_command: &str,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut command = git_cmd(git_command);
    command.arg("-C").arg(source).args(["worktree", "add"]);
    if create_branch {
        // 基于 origin/xxx 建分支时 git 默认会跟踪它，之后 `git push` 会因上游名不一致被拒。
        command
            .arg("--no-track")
            .arg("-b")
            .arg(branch)
            .arg(target)
            .arg(base_ref);
    } else {
        command.arg(target).arg(branch);
    }
    let output = command
        .output()
        .map_err(|error| tr!("无法启动 Git：{error}", error = error))?;
    if output.status.success() {
        crate::hook_runtime::link_worktree_runtime(source, target);
        return Ok(());
    }
    let _ = git_cmd(git_command)
        .arg("-C")
        .arg(source)
        .args(["worktree", "remove", "--force"])
        .arg(target)
        .output();
    if create_branch {
        let _ = git_cmd(git_command)
            .arg("-C")
            .arg(source)
            .args(["branch", "-D", branch])
            .output();
    }
    Err(tr!(
        "创建隔离工作区失败：{detail}",
        detail = String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        let destination = target.join(entry.file_name());
        if file_type.is_symlink() {
            copy_symlink(&entry.path(), &destination)?;
            continue;
        }
        if file_type.is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// 复制链接本身而不是跟随（如 `node_modules/.bin/*`）；Windows 建链接需要特权，直接跳过。
fn copy_symlink(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let link = fs::read_link(source).map_err(|error| error.to_string())?;
        std::os::unix::fs::symlink(link, destination).map_err(|error| error.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = (source, destination);
    Ok(())
}

fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::{create_worktree, discover, short_id};

    #[test]
    fn short_ids_are_safe_for_branch_names() {
        assert_eq!(short_id("123456789"), "12345678");
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn git_worktree_does_not_switch_source_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("isolated");
        fs::create_dir(&source).unwrap();
        git(&source, &["init"]);
        git(&source, &["config", "user.email", "test@multara.local"]);
        git(&source, &["config", "user.name", "Multara Test"]);
        fs::write(source.join("README.md"), "source").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-m", "initial"]);
        let original_branch = output(&source, &["branch", "--show-current"]);

        create_worktree(
            &source,
            &target,
            "multara/test/worktree",
            "HEAD",
            true,
            "git",
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(target.join("README.md")).unwrap(),
            "source"
        );
        assert_eq!(
            output(&source, &["branch", "--show-current"]),
            original_branch
        );
        git(
            &source,
            &["worktree", "remove", "--force", target.to_str().unwrap()],
        );
        git(&source, &["branch", "-D", "multara/test/worktree"]);
    }

    #[test]
    fn git_worktree_can_check_out_existing_branch() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("isolated");
        fs::create_dir(&source).unwrap();
        git(&source, &["init"]);
        git(&source, &["config", "user.email", "test@multara.local"]);
        git(&source, &["config", "user.name", "Multara Test"]);
        fs::write(source.join("README.md"), "source").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-m", "initial"]);
        git(&source, &["branch", "release"]);

        create_worktree(&source, &target, "release", "HEAD", false, "git").unwrap();

        assert_eq!(output(&target, &["branch", "--show-current"]), "release");
        git(
            &source,
            &["worktree", "remove", "--force", target.to_str().unwrap()],
        );
    }

    #[test]
    fn discovery_returns_git_branches_for_selection() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        git(&source, &["init"]);
        git(&source, &["config", "user.email", "test@multara.local"]);
        git(&source, &["config", "user.name", "Multara Test"]);
        fs::write(source.join("README.md"), "source").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-m", "initial"]);
        git(&source, &["branch", "release"]);

        let candidates = discover(source.to_str().unwrap(), "git").unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, "git");
        assert!(candidates[0]
            .branches
            .iter()
            .any(|branch| branch == "release"));
        assert_eq!(
            candidates[0].current_branch.as_deref(),
            Some(output(&source, &["branch", "--show-current"]).as_str())
        );
    }

    fn git(cwd: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn output(cwd: &std::path::Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }
}
