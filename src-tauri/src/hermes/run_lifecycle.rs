//! 收敛后 worktree 清理的纯决策逻辑（Phase 3d / Task 12）。
//!
//! 一次 run 收敛（Completed/Failed/Cancelled）后，对每个 task 的 worktree 做安全清扫。
//! 本模块只做**纯决策**（不碰 git / 文件系统）：根据 task 状态 + 是否有未提交改动 / 领先提交，
//! 决定 Remove（安全删）还是 RetainForReview（保留待用户在驾驶舱选 merge/discard）。
//!
//! 破坏性安全红线：**绝不静默删除未合并的工作**。完成且有产出 → 保留；失败/无改动 → 删除。
//! 实际删除前的双保险复查（has_uncommitted_changes 再查一次）由 Task 13 的 sweep 落地。
//!
//! 关于 `TaskStatus::Cancelled`：`TaskStatus` 枚举**没有** `Cancelled` 变体——
//! 「任务」不会被 cancel，「run」才会（见 `RunStatus::Cancelled`）。当 run 被 cancel 时，
//! 仍在飞的 task 停留在 `Dispatched`。因此 brief 中「Cancelled+有未提交→RetainForReview」
//! 的安全意图，落到 `Dispatched + has_uncommitted_changes → RetainForReview` 这条规则上
//! （保守保留中间产物，不丢用户可能想要的工作）。

use crate::hermes::types::TaskStatus;
use std::path::Path;
use crate::chat::{WorktreeManager, HELM_BRANCH_PREFIX};
use crate::hermes::events::{OrchestrationEvent, OrchestrationEventSink};
use crate::hermes::store::{Store, TaskListFilter};

/// 驾驶舱事件专用的 task 状态 token：worktree 保留待用户 merge/discard。
/// 非 TaskStatus 枚举值（DB 无此状态）；仅作为 OrchestrationEvent::Task 的 status 字段。
const TASK_STATUS_AWAITING_MERGE: &str = "awaiting-merge";

/// 单个 worktree 的清理处置（纯决策，不碰 git）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeDisposition {
    /// 安全删除（task 失败/取消，或无任何改动）。
    Remove,
    /// 保留，等用户在驾驶舱选 merge/discard（task 完成且有改动/领先提交，或 in-flight 有未提交）。
    RetainForReview,
}

/// 决策输入（纯数据，便于单测；has_uncommitted/has_commits_ahead 由调用方注入，不在此查 git）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeCleanupInput {
    pub task_status: TaskStatus,
    pub has_uncommitted_changes: bool,
    /// 相对 feat/helm 有领先提交 = 有产出。
    pub has_commits_ahead: bool,
}

/// 决策规则：
/// - Completed + (有改动 OR 领先提交) → RetainForReview（有产出，绝不静默删）。
/// - Completed + 无改动无领先 → Remove。
/// - Failed → Remove（失败工作；Task 13 删除前会再复查 has_uncommitted，有改动则降级 RetainForReview）。
/// - Dispatched（run 被 cancel 时 in-flight）+ 有未提交 → RetainForReview（保守，不丢中间产物）。
/// - Dispatched + 无改动 → Remove。
/// - Pending/Ready/Blocked（未派发，通常无 worktree）→ Remove。
pub fn decide_disposition(input: &WorktreeCleanupInput) -> WorktreeDisposition {
    match input.task_status {
        TaskStatus::Completed => {
            if input.has_uncommitted_changes || input.has_commits_ahead {
                WorktreeDisposition::RetainForReview
            } else {
                WorktreeDisposition::Remove
            }
        }
        TaskStatus::Dispatched => {
            // run 被 cancel 时仍在飞的 task：有未提交改动则保守保留。
            if input.has_uncommitted_changes {
                WorktreeDisposition::RetainForReview
            } else {
                WorktreeDisposition::Remove
            }
        }
        TaskStatus::Failed
        | TaskStatus::Pending
        | TaskStatus::Ready
        | TaskStatus::Blocked => WorktreeDisposition::Remove,
    }
}

/// sweep 结果摘要。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SweepReport {
    pub removed: usize,
    pub retained: usize,
}

/// 收敛后清扫 run 内所有 task 的 worktree（介质无关；用 WorktreeManager 关联函数 + repo_root）。
///
/// 流程：枚举 run 内 task → 按 helm/<task_id> 定位其 worktree（WorktreeManager::list）→
/// 查 has_uncommitted_changes / has_commits_ahead → decide_disposition →
/// Remove 调 WorktreeManager::remove(force=true)（**删除前再复查 has_uncommitted_changes**，
/// 意外有改动则降级 RetainForReview，绝不删未保存工作）；RetainForReview 发 Task{awaiting-merge} 事件。
pub fn sweep_run_worktrees(
    repo_root: &Path,
    store: &Store,
    base_branch: &str,
    sink: &dyn OrchestrationEventSink,
    run_id: &str,
) -> Result<SweepReport, String> {
    let mut removed = 0usize;
    let mut retained = 0usize;
    // 1. 枚举所有 task（TaskListFilter::default 不带过滤，列出全部）。
    let tasks = store.list_tasks(TaskListFilter::default())?;
    // 2. 枚举所有 worktree（按分支名 helm/<task_id> 匹配 task）。
    let worktrees = WorktreeManager::list(repo_root)?;
    for task in &tasks {
        let expected_branch = format!("{}{}", HELM_BRANCH_PREFIX, task.id);
        // 找该 task 的 worktree（按 branch 匹配）。
        let Some(wt) = worktrees.iter().find(|w| w.branch == expected_branch) else {
            continue; // 无 worktree（未派发过）→ 跳过。
        };
        // 3. 查 has_uncommitted / has_commits_ahead。
        // 破坏性安全红线：has_uncommitted_changes 在 git 出错时 unwrap_or(true)——
        // 「查不动」按「有改动」处理（fail-safe，宁可不删也不误删未保存工作）。
        // has_commits_ahead 保持 unwrap_or(false)（它是「有产出」信号，error→false 无碍；
        //   且 Completed 下 has_uncommitted 已先守门，has_commits_ahead 的 error 方向不改变安全语义）。
        let has_uncommitted =
            WorktreeManager::has_uncommitted_changes(&wt.path).unwrap_or(true);
        let has_commits_ahead =
            WorktreeManager::has_commits_ahead(&wt.path, base_branch).unwrap_or(false);
        let input = WorktreeCleanupInput {
            task_status: task.status,
            has_uncommitted_changes: has_uncommitted,
            has_commits_ahead,
        };
        let disposition = decide_disposition(&input);
        match disposition {
            WorktreeDisposition::Remove => {
                // 破坏性安全双保险：删除前再复查 has_uncommitted_changes。
                // 即使纯决策说 Remove（如 Failed），意外有未提交改动则降级 RetainForReview。
                // git 出错也按「脏」处理（unwrap_or(true) fail-safe）：查不动 → 不删 → 保留待查。
                if WorktreeManager::has_uncommitted_changes(&wt.path).unwrap_or(true) {
                    sink.emit(OrchestrationEvent::Task {
                        run_id: run_id.to_string(),
                        task_id: task.id.clone(),
                        status: TASK_STATUS_AWAITING_MERGE.to_string(),
                        dispatch_id: None,
                    });
                    retained += 1;
                } else {
                    // remove 失败（worktree 锁/元数据异常）不静默漏删——
                    // 保守计 retained，提示该 worktree 仍在、需人工处理。
                    if WorktreeManager::remove(repo_root, &wt.path, true).is_ok() {
                        removed += 1;
                    } else {
                        retained += 1;
                    }
                }
            }
            WorktreeDisposition::RetainForReview => {
                sink.emit(OrchestrationEvent::Task {
                    run_id: run_id.to_string(),
                    task_id: task.id.clone(),
                    status: TASK_STATUS_AWAITING_MERGE.to_string(),
                    dispatch_id: None,
                });
                retained += 1;
            }
        }
    }
    Ok(SweepReport { removed, retained })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hermes::types::TaskStatus;

    /// 构造一个全部字段默认为 false 的输入，便于单测里只翻转关心的位。
    fn input(status: TaskStatus) -> WorktreeCleanupInput {
        WorktreeCleanupInput {
            task_status: status,
            has_uncommitted_changes: false,
            has_commits_ahead: false,
        }
    }

    // ── Completed ──

    #[test]
    fn completed_with_commits_ahead_is_retain() {
        let mut i = input(TaskStatus::Completed);
        i.has_commits_ahead = true;
        assert_eq!(decide_disposition(&i), WorktreeDisposition::RetainForReview);
    }

    #[test]
    fn completed_with_uncommitted_changes_is_retain() {
        let mut i = input(TaskStatus::Completed);
        i.has_uncommitted_changes = true;
        // 无领先提交，但有未提交改动 → 仍有产出，保留。
        assert_eq!(decide_disposition(&i), WorktreeDisposition::RetainForReview);
    }

    #[test]
    fn completed_with_no_output_is_remove() {
        let i = input(TaskStatus::Completed);
        assert_eq!(decide_disposition(&i), WorktreeDisposition::Remove);
    }

    // ── Failed（纯逻辑一律 Remove；破坏性双保险复查在 Task 13）──

    #[test]
    fn failed_with_changes_is_remove_in_pure_logic() {
        let mut i = input(TaskStatus::Failed);
        i.has_uncommitted_changes = true;
        i.has_commits_ahead = true;
        assert_eq!(decide_disposition(&i), WorktreeDisposition::Remove);
    }

    #[test]
    fn failed_without_changes_is_remove() {
        let i = input(TaskStatus::Failed);
        assert_eq!(decide_disposition(&i), WorktreeDisposition::Remove);
    }

    // ── Dispatched（run 被 cancel 时仍在飞的 task）──

    #[test]
    fn dispatched_with_uncommitted_changes_is_retain() {
        // 对应 brief 中「Cancelled+有未提交→RetainForReview」：TaskStatus 无 Cancelled，
        // run 被 cancel 时仍在飞的 task 状态为 Dispatched，保守保留中间产物。
        let mut i = input(TaskStatus::Dispatched);
        i.has_uncommitted_changes = true;
        assert_eq!(decide_disposition(&i), WorktreeDisposition::RetainForReview);
    }

    #[test]
    fn dispatched_without_changes_is_remove() {
        let i = input(TaskStatus::Dispatched);
        assert_eq!(decide_disposition(&i), WorktreeDisposition::Remove);
    }

    // ── 未派发状态（通常无 worktree）──

    #[test]
    fn pending_is_remove() {
        assert_eq!(decide_disposition(&input(TaskStatus::Pending)), WorktreeDisposition::Remove);
    }

    #[test]
    fn ready_is_remove() {
        assert_eq!(decide_disposition(&input(TaskStatus::Ready)), WorktreeDisposition::Remove);
    }

    #[test]
    fn blocked_is_remove() {
        assert_eq!(decide_disposition(&input(TaskStatus::Blocked)), WorktreeDisposition::Remove);
    }

    // ── 边界：Completed 同时有改动 + 领先提交（仍 Retain，不重复删）──

    #[test]
    fn completed_with_both_changes_and_commits_is_retain() {
        let mut i = input(TaskStatus::Completed);
        i.has_uncommitted_changes = true;
        i.has_commits_ahead = true;
        assert_eq!(decide_disposition(&i), WorktreeDisposition::RetainForReview);
    }

    // ── Task 13：sweep_run_worktrees（真 tempfile git repo，不 mock 关联函数）──
    use std::path::Path;
    use std::process::Command;
    use std::sync::{Arc, Mutex};
    use crate::chat::WorktreeManager;
    use crate::hermes::events::{OrchestrationEvent, OrchestrationEventSink};
    use crate::hermes::store::Store;
    use crate::hermes::types::Task;

    /// 真 git repo 辅助：在 dir 里跑 git，失败即 panic（镜像 worktree.rs::tests）。
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

    /// 捕获 repo 当前分支名（git init 默认 main/master 跨系统不一）。
    fn current_branch(repo: &Path) -> String {
        String::from_utf8_lossy(
            &Command::new("git")
                .current_dir(repo)
                .args(["symbolic-ref", "--short", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string()
    }

    /// 收集型 sink：把 emit 的事件收集到 Vec，断言用（镜像 coordinator.rs::tests）。
    struct CollectSink(Mutex<Vec<OrchestrationEvent>>);
    impl OrchestrationEventSink for CollectSink {
        fn emit(&self, ev: OrchestrationEvent) {
            self.0.lock().unwrap().push(ev);
        }
    }
    impl CollectSink {
        fn snapshot(&self) -> Vec<OrchestrationEvent> {
            self.0.lock().unwrap().clone()
        }
    }

    fn sample_task_for_sweep(id: &str) -> Task {
        Task {
            id: id.to_string(),
            parent_id: None,
            spec: format!("spec for {id}"),
            status: TaskStatus::Pending, // create_task 会按 deps 推导
            deps: vec![],
            result: None,
            assignment: None,
            created_at: "2026-06-28T00:00:00Z".to_string(),
            completed_at: None,
        }
    }

    #[test]
    fn sweep_run_worktrees_removes_clean_and_retains_with_commits() {
        // 真 tempfile git repo：task1 = Completed+领先提交 → RetainForReview；
        // task2 = Failed+干净 → Remove。断言 SweepReport{removed:1, retained:1}。
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let base = current_branch(&repo);
        let wts_dir = tmp.path().join("worktrees");

        let store = Store::open_in_memory().unwrap();

        // task1 = Completed，建 worktree + commit 一次（领先）→ RetainForReview。
        store.create_task(sample_task_for_sweep("task1")).unwrap();
        let wt1 = WorktreeManager::create(&repo, &wts_dir, "task1").unwrap();
        commit_file(&wt1.path, "feat1.txt", "done", "feat1");
        store
            .update_task_status("task1", TaskStatus::Completed, None)
            .unwrap();

        // task2 = Failed，建 worktree 但保持干净 → Remove。
        store.create_task(sample_task_for_sweep("task2")).unwrap();
        let wt2 = WorktreeManager::create(&repo, &wts_dir, "task2").unwrap();
        store
            .update_task_status("task2", TaskStatus::Failed, None)
            .unwrap();

        let sink = Arc::new(CollectSink(Mutex::new(Vec::new())));
        let report =
            sweep_run_worktrees(&repo, &store, &base, sink.as_ref(), "run_x").unwrap();

        assert_eq!(report, SweepReport { removed: 1, retained: 1 });
        assert!(wt1.path.exists(), "有产出的 worktree 必须保留");
        assert!(!wt2.path.exists(), "干净的 Failed worktree 必须删除");

        // 验证事件：一个 awaiting-merge（task1）。
        let evs = sink.snapshot();
        let awaiting: Vec<_> = evs
            .iter()
            .filter(|e| match e {
                OrchestrationEvent::Task { status, .. } => {
                    status.as_str() == TASK_STATUS_AWAITING_MERGE
                }
                _ => false,
            })
            .collect();
        assert_eq!(awaiting.len(), 1, "应发一个 awaiting-merge 事件");
    }

    #[test]
    fn sweep_downgrades_failed_dirty_worktree_to_retain() {
        // 破坏性安全双保险：Failed task 但 worktree 意外有未提交改动 → 降级 RetainForReview。
        // 纯决策对 Failed 一律 Remove，但删除前复查 has_uncommitted_changes 要保留。
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let base = current_branch(&repo);
        let wts_dir = tmp.path().join("worktrees");

        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task_for_sweep("task3")).unwrap();
        let wt3 = WorktreeManager::create(&repo, &wts_dir, "task3").unwrap();
        // 意外脏：写未提交文件。
        std::fs::write(wt3.path.join("uncommitted.txt"), "dirty").unwrap();
        store
            .update_task_status("task3", TaskStatus::Failed, None)
            .unwrap();

        let sink = Arc::new(CollectSink(Mutex::new(Vec::new())));
        let report =
            sweep_run_worktrees(&repo, &store, &base, sink.as_ref(), "run_y").unwrap();

        assert_eq!(report, SweepReport { removed: 0, retained: 1 });
        assert!(wt3.path.exists(), "脏 worktree 绝不删（双保险降级）");

        let evs = sink.snapshot();
        assert!(
            evs.iter().any(|e| matches!(e,
                OrchestrationEvent::Task { task_id, status, .. }
                if task_id == "task3" && status.as_str() == TASK_STATUS_AWAITING_MERGE)),
            "应发 task3 awaiting-merge 事件"
        );
    }

    #[test]
    fn sweep_retains_when_uncommitted_check_errors_fail_safe() {
        // 破坏性安全 fail-safe：删除前双保险复查 has_uncommitted_changes 返回 Err
        // （worktree 元数据损坏 / index.lock 争用 / 权限等）时，按「脏」处理 → RetainForReview。
        // 构造：Failed task + 真 worktree，然后删掉 worktree 的 .git 文件 →
        //   `git status --porcelain` 在该路径下 fatal（exit 128）→ has_uncommitted_changes 返回 Err；
        //   `git worktree list --porcelain` 从 repo_root 仍能列出该 worktree（读主仓 .git/worktrees/ 元数据），
        //   故 sweep 的 find 仍命中 → 走到双保险复查 → Err 被 unwrap_or(true) 拦下 → RetainForReview。
        // 断言 fail-safe 方向：git 错误时「不删」而非「删」。
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let base = current_branch(&repo);
        let wts_dir = tmp.path().join("worktrees");

        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task_for_sweep("task4")).unwrap();
        let wt4 = WorktreeManager::create(&repo, &wts_dir, "task4").unwrap();
        // Failed + 干净 → decide_disposition 说 Remove；但下面要破坏 git 查询能力。
        store
            .update_task_status("task4", TaskStatus::Failed, None)
            .unwrap();

        // 损坏 worktree 的 git 元数据：删除 .git 文件（worktree 的 .git 是文件不是目录，
        // 指向主仓 .git/worktrees/<name>/）。删后该路径下 `git status` fatal → Err。
        let dot_git = wt4.path.join(".git");
        assert!(dot_git.exists(), "worktree 应有 .git 文件");
        std::fs::remove_file(&dot_git).unwrap();
        // 确认 has_uncommitted_changes 现在确实 Err（校验测试前提）。
        assert!(
            WorktreeManager::has_uncommitted_changes(&wt4.path).is_err(),
            "破坏后 has_uncommitted_changes 必须返回 Err"
        );

        let sink = Arc::new(CollectSink(Mutex::new(Vec::new())));
        let report =
            sweep_run_worktrees(&repo, &store, &base, sink.as_ref(), "run_z").unwrap();

        // fail-safe：git 错误 → 不删 → retained +1，removed 不增。
        assert_eq!(report, SweepReport { removed: 0, retained: 1 });
        // worktree 目录仍在（没被删）。
        assert!(wt4.path.exists(), "git 查询出错时 worktree 必须 fail-safe 保留");

        let evs = sink.snapshot();
        assert!(
            evs.iter().any(|e| matches!(e,
                OrchestrationEvent::Task { task_id, status, .. }
                if task_id == "task4" && status.as_str() == TASK_STATUS_AWAITING_MERGE)),
            "git 出错应发 task4 awaiting-merge 事件（fail-safe 保留）"
        );
    }
}
