//! 每 Agent 独立 git worktree 的管理：建立/删除/列举/脏检查。
//! worktree 是 Helm 并行隔离的物理边界——多个 Agent 改同一 repo 互不踩。

use std::path::{Path, PathBuf};
use std::process::Command;

/// Helm 创建的分支前缀，集中常量避免魔法串。
pub const HELM_BRANCH_PREFIX: &str = "helm/";

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiffSummary {
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged,
    Conflict,
}

pub struct WorktreeManager;

impl WorktreeManager {
    fn run(repo_root: &Path, args: &[&str]) -> Result<String, String> {
        let out = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()
            .map_err(|e| format!("git 执行失败: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git {:?} 失败: {}",
                args,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// 在 worktrees_dir/name 建立新分支 helm/<name> 的 worktree（基线 = repo HEAD）。
    pub fn create(
        repo_root: &Path,
        worktrees_dir: &Path,
        name: &str,
    ) -> Result<WorktreeInfo, String> {
        std::fs::create_dir_all(worktrees_dir)
            .map_err(|e| format!("创建 worktrees 目录失败: {e}"))?;
        let path = worktrees_dir.join(name);
        let branch = format!("{HELM_BRANCH_PREFIX}{name}");
        let path_str = path.to_string_lossy();
        Self::run(repo_root, &["worktree", "add", "-b", &branch, &path_str, "HEAD"])?;
        let path = path.canonicalize().unwrap_or(path);
        Ok(WorktreeInfo { path, branch })
    }

    /// 删除 worktree。非 force 时若有未提交改动则拒绝。
    pub fn remove(repo_root: &Path, worktree_path: &Path, force: bool) -> Result<(), String> {
        if !force && Self::has_uncommitted_changes(worktree_path)? {
            return Err("worktree 有未提交改动，拒绝删除（需显式 force）".into());
        }
        let path_str = worktree_path.to_string_lossy();
        if force {
            Self::run(repo_root, &["worktree", "remove", "--force", &path_str])?;
        } else {
            Self::run(repo_root, &["worktree", "remove", &path_str])?;
        }
        Ok(())
    }

    /// 列出 repo 的所有 worktree（解析 `git worktree list --porcelain`）。
    pub fn list(repo_root: &Path) -> Result<Vec<WorktreeInfo>, String> {
        let out = Self::run(repo_root, &["worktree", "list", "--porcelain"])?;
        let mut result = Vec::new();
        let mut cur_path: Option<PathBuf> = None;
        for line in out.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                let path = PathBuf::from(p.trim());
                cur_path = Some(path.canonicalize().unwrap_or(path));
            } else if let Some(b) = line.strip_prefix("branch ") {
                if let Some(path) = cur_path.take() {
                    let branch = b
                        .trim()
                        .strip_prefix("refs/heads/")
                        .unwrap_or(b.trim())
                        .to_string();
                    result.push(WorktreeInfo { path, branch });
                }
            } else if line.is_empty() {
                cur_path = None;
            }
        }
        Ok(result)
    }

    /// 该 worktree 是否有未提交改动（含未跟踪文件）。
    pub fn has_uncommitted_changes(worktree_path: &Path) -> Result<bool, String> {
        let out = Self::run(worktree_path, &["status", "--porcelain"])?;
        Ok(!out.trim().is_empty())
    }

    /// 该 worktree 相对 base_branch 是否有领先提交（有产出）。
    /// `git -C <worktree> rev-list --count <base>..HEAD` > 0。
    /// Task 13（3d）：sweep_run_worktrees 用它判断 Completed task 是否有产出 → RetainForReview。
    pub fn has_commits_ahead(worktree_path: &Path, base_branch: &str) -> Result<bool, String> {
        // rev-list --count base..HEAD：HEAD 领先 base 的提交数。0=无产出。
        // worktree_path 作为 run() 的 repo_root 参数（与 has_uncommitted_changes 同模式），
        // 让 git 在 worktree 目录里执行；<base>..HEAD 在同一 repo 的任意 worktree 均可解析。
        let range = format!("{base_branch}..HEAD");
        let out = Self::run(worktree_path, &["rev-list", "--count", &range])?;
        let count: u64 = out.trim().parse().unwrap_or(0);
        Ok(count > 0)
    }

    /// 相对 HEAD 的改动摘要（含已跟踪文件改动；解析 `git diff --shortstat`）。
    pub fn diff_summary(worktree_path: &Path) -> Result<DiffSummary, String> {
        // 把未跟踪文件登记为 intent-to-add，使其纳入 `git diff` 统计；读完即还原。
        let _ = Self::run(worktree_path, &["add", "--intent-to-add", "--", "."]);
        let out = Self::run(worktree_path, &["diff", "--shortstat", "HEAD"]);
        // 无论统计成功与否都还原 intent-to-add，避免污染暂存区。
        let _ = Self::run(worktree_path, &["reset", "--quiet"]);
        let out = out?;
        let mut summary = DiffSummary::default();
        for part in out.split(',') {
            let item = part.trim();
            let count: u32 = item
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            if item.contains("file") {
                summary.files_changed = count;
            } else if item.contains("insertion") {
                summary.insertions = count;
            } else if item.contains("deletion") {
                summary.deletions = count;
            }
        }
        Ok(summary)
    }

    /// 把 source_branch 合并进 repo_root 当前分支。冲突则 abort 回滚，返回 Conflict。
    pub fn merge_into_current(repo_root: &Path, source_branch: &str) -> Result<MergeOutcome, String> {
        let out = Command::new("git")
            .current_dir(repo_root)
            .args(["merge", "--no-ff", source_branch])
            .output()
            .map_err(|e| format!("git merge 执行失败: {e}"))?;
        if out.status.success() {
            return Ok(MergeOutcome::Merged);
        }

        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        // 任何 merge 失败都尝试回滚，避免主仓留在冲突态或半完成状态。
        let _ = Command::new("git")
            .current_dir(repo_root)
            .args(["merge", "--abort"])
            .output();
        if combined.contains("CONFLICT") || combined.contains("conflict") {
            Ok(MergeOutcome::Conflict)
        } else {
            Err(format!("git merge 失败: {}", combined.trim()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorktreeManager;
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {:?} failed", args);
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "t@t.t"]);
        git(dir, &["config", "user.name", "t"]);
        std::fs::write(dir.join("README.md"), "hi").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-qm", "init"]);
    }

    fn commit_file(dir: &Path, name: &str, content: &str, msg: &str) {
        std::fs::write(dir.join(name), content).unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-qm", msg]);
    }

    #[test]
    fn create_then_list_then_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");

        let info = WorktreeManager::create(&repo, &wts, "task-a").unwrap();
        assert!(info.path.exists());
        assert_eq!(info.branch, "helm/task-a");
        assert!(
            info.path.join("README.md").exists(),
            "worktree 是完整 checkout"
        );

        let listed = WorktreeManager::list(&repo).unwrap();
        assert!(listed.iter().any(|w| w.path == info.path));

        assert!(!WorktreeManager::has_uncommitted_changes(&info.path).unwrap());

        WorktreeManager::remove(&repo, &info.path, false).unwrap();
        assert!(!info.path.exists());
    }

    #[test]
    fn remove_without_force_refuses_dirty_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "task-b").unwrap();

        std::fs::write(info.path.join("new.txt"), "dirty").unwrap();
        assert!(WorktreeManager::has_uncommitted_changes(&info.path).unwrap());
        assert!(
            WorktreeManager::remove(&repo, &info.path, false).is_err(),
            "脏工作树非 force 必须拒删"
        );
        assert!(
            WorktreeManager::remove(&repo, &info.path, true).is_ok(),
            "force 可删"
        );
    }

    #[test]
    fn diff_summary_counts_changes_vs_head() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "task-c").unwrap();

        std::fs::write(info.path.join("README.md"), "hi\nmore\n").unwrap();
        let s = WorktreeManager::diff_summary(&info.path).unwrap();
        assert!(s.files_changed >= 1);
        assert!(s.insertions >= 1);
    }

    #[test]
    fn diff_summary_includes_untracked_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "task-u").unwrap();

        std::fs::write(info.path.join("brand_new.txt"), "a\nb\n").unwrap();
        let s = WorktreeManager::diff_summary(&info.path).unwrap();
        assert!(s.files_changed >= 1, "未跟踪新文件应计入 files_changed");
        assert!(s.insertions >= 2, "未跟踪新文件行数应计入 insertions");
    }

    #[test]
    fn has_commits_ahead_detects_commits_past_base() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        // 捕获默认分支名（main / master 跨系统不一），作为 worktree 的 base。
        let base = String::from_utf8_lossy(
            &Command::new("git")
                .current_dir(&repo)
                .args(["symbolic-ref", "--short", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let wts = tmp.path().join("worktrees");

        // 干净 worktree（无领先提交）→ false。
        let clean = WorktreeManager::create(&repo, &wts, "ahead-clean").unwrap();
        assert!(
            !WorktreeManager::has_commits_ahead(&clean.path, &base).unwrap(),
            "新建未提交的 worktree 不应领先 base"
        );

        // 在 worktree 里 commit 一次 → 领先 base → true。
        commit_file(&clean.path, "feat.txt", "done", "feat");
        assert!(
            WorktreeManager::has_commits_ahead(&clean.path, &base).unwrap(),
            "worktree 提交后应领先 base"
        );
    }

    #[test]
    fn merge_clean_brings_worktree_commit_into_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "win").unwrap();

        commit_file(&info.path, "feature.txt", "done", "feat");

        let outcome = WorktreeManager::merge_into_current(&repo, &info.branch).unwrap();
        assert!(matches!(outcome, super::MergeOutcome::Merged));
        assert!(repo.join("feature.txt").exists(), "赢家改动已合入主仓");
    }

    #[test]
    fn merge_conflict_aborts_and_leaves_repo_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "conf").unwrap();

        commit_file(&info.path, "README.md", "from-worktree", "wt change");
        commit_file(&repo, "README.md", "from-main", "main change");

        let outcome = WorktreeManager::merge_into_current(&repo, &info.branch).unwrap();
        assert!(matches!(outcome, super::MergeOutcome::Conflict));
        assert!(
            !WorktreeManager::has_uncommitted_changes(&repo).unwrap(),
            "冲突已 abort，主仓干净"
        );
    }
}
