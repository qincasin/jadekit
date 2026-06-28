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
}
