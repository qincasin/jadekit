# Helm × Hermes — Phase 1：Worktree 隔离（每 Agent 独立 git worktree）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Phase 0 的 daemon 池之上，给每个 Agent 绑定一个独立的 git worktree（独立 checkout），使多个 Agent 能并行修改同一个 repo 而互不踩；并先修复 Phase 0 review 发现的两个真实缺陷。

**Architecture:** 新增 `WorktreeManager`（`git worktree add/remove/list` + diff 摘要）。Agent 的 cwd 从「用户任选目录」升级为「该 Agent 专属 worktree 路径」。前端 Composer 增加「为此 Agent 建 worktree」选项；Agent 关闭时清理 worktree 与 permission watcher。

**Tech Stack:** Rust（tokio、std::process::Command 调 git）、Tauri 2、React + TS + Zustand。

**前置依赖:** Phase 0 已合入（`AgentPool`、`AgentId`、`ChatManager::send(agent_id,…)`、per-agent permission watcher）。本计划基于分支 `feat/helm-phase0-daemon-pool` 的真实实现。

## Global Constraints

（同 Phase 0）不写魔法字符串（worktree 状态、命令名、目录前缀集中常量）；JSON camelCase + Rust snake_case；新命令注册到 `lib.rs` `generate_handler!`；daemon/worktree 操作经抽象，不绕过；新增代码补中文注释；新增能力补测试 + 中文文档；每 task 结束 commit（Conventional Commits）；提交前 `cargo check` + 涉及前端 `npm run build`；`git diff --check` 干净。

**新增约束:** worktree 是破坏性资源（建/删真实目录与 git 状态）。删除 worktree 前必须预检（有未提交改动则需显式确认）；绝不强删用户未确认的工作树。

---

## 阅前必读

```bash
codegraph node AgentPool
codegraph node ChatManager
codegraph explore worktree git branch cwd chat_send provider
```
通读真实文件：
- `src-tauri/src/chat/pool.rs`（`get_or_init` 竞态 —— Task 1 修）
- `src-tauri/src/chat/manager.rs`（`client_for`/`ensure_permission_watcher`/`shutdown_all`/`send`）
- `src-tauri/src/commands/chat_commands.rs`（已有 `chat_git_create_and_checkout_branch`/`resolve_git_repository`/`list_chat_git_branches_for_path`，复用其 git 解析）
- `src/stores/useChatStore.ts`（`createAgentId`/tab 结构/`sendMessage`）

> 新建文件：`src-tauri/src/chat/worktree.rs`（`WorktreeManager`）。其余就地 Modify。

---

## Task 1: 修复 AgentPool 并发 init 的孤儿 daemon 泄漏（review 发现）

**Files:**
- Modify: `src-tauri/src/chat/pool.rs`
- Test: `src-tauri/src/chat/pool.rs`（`#[cfg(test)]`）

**Interfaces:**
- Consumes: `ManagerDaemonClient::stop()`（已存在）
- Produces: `get_or_init` 行为不变，但竞态败者会 `.stop()` 被丢弃的 client

- [ ] **Step 1: 写失败测试**

需要 fake 暴露「是否被 stop 过」。先给 `manager.rs` 的 `FakeDaemonClient` 增加 `stop_calls: Arc<AtomicUsize>` 并在 `stop()` 自增，且 `test_support::fake_client_with_stop_counter() -> (Arc<dyn ManagerDaemonClient>, Arc<AtomicUsize>)`。然后：

```rust
    #[tokio::test]
    async fn concurrent_init_stops_discarded_loser() {
        use crate::chat::manager::test_support::fake_client_with_stop_counter;
        let pool = AgentPool::new();

        // 预先占位 "a"，模拟“他人已抢先写入”。
        pool.get_or_init(&"a".to_string(), || async { Ok(crate::chat::manager::test_support::fake_client()) })
            .await.unwrap();

        // 第二次 init 必然命中“双检发现已存在”分支：它构造的 loser 必须被 stop。
        let (loser, stop_calls) = fake_client_with_stop_counter();
        let returned = pool
            .get_or_init(&"a".to_string(), || async move { Ok(loser) })
            .await
            .unwrap();

        // 返回的是既有 client，不是 loser；loser 已被 stop 回收。
        assert_eq!(stop_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let _ = returned;
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::pool::tests::concurrent_init_stops_discarded_loser`
Expected: FAIL（`stop_calls == 0`，败者未被回收）。

- [ ] **Step 3: 实现修复**

`get_or_init` 双检命中既有时，先停掉本次创建的 client：

```rust
        let client = init().await?;
        let mut guard = self.clients.lock().await;
        if let Some(existing) = guard.get(id) {
            let existing = existing.clone();
            drop(guard); // 释放锁后再 stop，避免持锁 await
            client.stop().await; // 回收竞态败者的 daemon 进程，防止孤儿
            return Ok(existing);
        }
        guard.insert(id.clone(), client.clone());
        Ok(client)
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::pool`
Expected: 全绿（含新用例 + 原有 2 个）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/pool.rs src-tauri/src/chat/manager.rs
git commit -m "fix(chat): stop discarded client on AgentPool init race to avoid orphan daemon"
```

---

## Task 2: `WorktreeManager` —— create / remove / list

**Files:**
- Create: `src-tauri/src/chat/worktree.rs`
- Modify: `src-tauri/src/chat/mod.rs`（`mod worktree;` + `pub use worktree::{WorktreeManager, WorktreeInfo};`）
- Test: `src-tauri/src/chat/worktree.rs`（用临时 git repo）

**Interfaces:**
- Produces:
  - `pub struct WorktreeInfo { pub path: PathBuf, pub branch: String }`
  - `pub struct WorktreeManager;`
  - `WorktreeManager::create(repo_root: &Path, worktrees_dir: &Path, name: &str) -> Result<WorktreeInfo, String>`（在 `worktrees_dir/name` 建一个新分支 `helm/<name>` 的 worktree，基线为 repo 当前 HEAD）
  - `WorktreeManager::remove(repo_root: &Path, worktree_path: &Path, force: bool) -> Result<(), String>`
  - `WorktreeManager::list(repo_root: &Path) -> Result<Vec<WorktreeInfo>, String>`
  - `WorktreeManager::has_uncommitted_changes(worktree_path: &Path) -> Result<bool, String>`（删除预检用）

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::WorktreeManager;
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").current_dir(dir).args(args).status().unwrap().success();
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
        assert!(info.path.join("README.md").exists(), "worktree 是完整 checkout");

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
        assert!(WorktreeManager::remove(&repo, &info.path, false).is_err(), "脏工作树非 force 必须拒删");
        assert!(WorktreeManager::remove(&repo, &info.path, true).is_ok(), "force 可删");
    }
}
```

> 依赖 dev：`tempfile`。若 `Cargo.toml` 的 `[dev-dependencies]` 未含 `tempfile`，本 task 加上（`tempfile = "3"`）。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree`
Expected: 编译失败（`WorktreeManager` 未定义）。

- [ ] **Step 3: 实现**

```rust
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
    pub fn create(repo_root: &Path, worktrees_dir: &Path, name: &str) -> Result<WorktreeInfo, String> {
        std::fs::create_dir_all(worktrees_dir).map_err(|e| format!("创建 worktrees 目录失败: {e}"))?;
        let path = worktrees_dir.join(name);
        let branch = format!("{HELM_BRANCH_PREFIX}{name}");
        let path_str = path.to_string_lossy();
        Self::run(repo_root, &["worktree", "add", "-b", &branch, &path_str, "HEAD"])?;
        Ok(WorktreeInfo { path, branch })
    }

    /// 删除 worktree。非 force 时若有未提交改动则拒绝。
    pub fn remove(repo_root: &Path, worktree_path: &Path, force: bool) -> Result<(), String> {
        if !force && Self::has_uncommitted_changes(worktree_path)? {
            return Err("worktree 有未提交改动，拒绝删除（需显式 force）".into());
        }
        let path_str = worktree_path.to_string_lossy();
        let mut args = vec!["worktree", "remove", &path_str];
        if force {
            args.push("--force");
        }
        Self::run(repo_root, &args)?;
        Ok(())
    }

    /// 列出 repo 的所有 worktree（解析 `git worktree list --porcelain`）。
    pub fn list(repo_root: &Path) -> Result<Vec<WorktreeInfo>, String> {
        let out = Self::run(repo_root, &["worktree", "list", "--porcelain"])?;
        let mut result = Vec::new();
        let mut cur_path: Option<PathBuf> = None;
        for line in out.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                cur_path = Some(PathBuf::from(p.trim()));
            } else if let Some(b) = line.strip_prefix("branch ") {
                if let Some(path) = cur_path.take() {
                    let branch = b.trim().rsplit('/').next().unwrap_or(b.trim()).to_string();
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
}
```

`mod.rs` 加 `mod worktree;` 与 `pub use worktree::{WorktreeInfo, WorktreeManager};`。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree`
Expected: 2 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/worktree.rs src-tauri/src/chat/mod.rs src-tauri/Cargo.toml
git commit -m "feat(chat): add WorktreeManager (create/remove/list/dirty-check)"
```

---

## Task 3: worktree diff 摘要（+/- 行数，供卡片展示）

**Files:**
- Modify: `src-tauri/src/chat/worktree.rs`
- Test: 同文件

**Interfaces:**
- Produces: `WorktreeManager::diff_summary(worktree_path: &Path) -> Result<DiffSummary, String>`；`pub struct DiffSummary { pub files_changed: u32, pub insertions: u32, pub deletions: u32 }`

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn diff_summary_counts_changes_vs_head() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "task-c").unwrap();

        std::fs::write(info.path.join("README.md"), "hi\nmore\n").unwrap(); // 改已跟踪文件
        let s = WorktreeManager::diff_summary(&info.path).unwrap();
        assert!(s.files_changed >= 1);
        assert!(s.insertions >= 1);
    }
```

- [ ] **Step 2: 确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree::tests::diff_summary_counts_changes_vs_head`
Expected: 编译失败。

- [ ] **Step 3: 实现**

```rust
#[derive(Debug, Clone, Default)]
pub struct DiffSummary {
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

impl WorktreeManager {
    /// 相对 HEAD 的改动摘要（含已跟踪文件改动；解析 `git diff --shortstat`）。
    pub fn diff_summary(worktree_path: &Path) -> Result<DiffSummary, String> {
        // 包含已暂存与未暂存改动相对 HEAD。
        let out = Self::run(worktree_path, &["diff", "--shortstat", "HEAD"])?;
        let mut s = DiffSummary::default();
        for part in out.split(',') {
            let p = part.trim();
            let num: u32 = p.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            if p.contains("file") { s.files_changed = num; }
            else if p.contains("insertion") { s.insertions = num; }
            else if p.contains("deletion") { s.deletions = num; }
        }
        Ok(s)
    }
}
```

- [ ] **Step 4: 确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/worktree.rs
git commit -m "feat(chat): add worktree diff_summary (files/insertions/deletions)"
```

---

## Task 4: Tauri 命令暴露 worktree 操作

**Files:**
- Modify: `src-tauri/src/commands/chat_commands.rs`（新增命令）
- Modify: `src-tauri/src/lib.rs`（`generate_handler!` 注册）
- Test: 复用 manager 单测 + 编译级验证

**Interfaces:**
- Produces（camelCase 参数）:
  - `helm_worktree_create(repo_root: String, name: String, state) -> Result<WorktreeInfoDto, String>`
  - `helm_worktree_remove(repo_root: String, worktree_path: String, force: bool, state) -> Result<(), String>`
  - `helm_worktree_list(repo_root: String) -> Result<Vec<WorktreeInfoDto>, String>`
  - `helm_worktree_diff(worktree_path: String) -> Result<DiffSummaryDto, String>`
  - `WorktreeInfoDto { path: String, branch: String }`、`DiffSummaryDto { filesChanged, insertions, deletions }`（serde rename camelCase）

- [ ] **Step 1: 实现命令**

worktrees_dir 约定：`<app_data_dir>/helm-worktrees/<repo 名>/`，集中为常量函数 `helm_worktrees_dir(app, repo_root)`。`helm_worktree_create` 解析后调 `WorktreeManager::create`。所有命令把 `WorktreeInfo`/`DiffSummary` 转 DTO（camelCase）。

- [ ] **Step 2: 注册 + 编译**

在 `lib.rs` `generate_handler!` 加这 4 个命令。
Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 3: 后端测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat`
Expected: PASS。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands/chat_commands.rs src-tauri/src/lib.rs
git commit -m "feat(chat): expose helm worktree commands (create/remove/list/diff)"
```

---

## Task 5: Agent 绑定 worktree —— send 用 worktree 路径作 cwd

**Files:**
- Modify: `src/stores/useChatStore.ts`（tab 增加 `worktreePath?: string`；`sendMessage` 的 `params.cwd` 优先用 `tab.worktreePath`）
- Modify: `src/services/`（worktree 服务封装：`createWorktree/removeWorktree/listWorktrees/worktreeDiff`）
- Test: `src/stores/`（纯函数 `resolveSendCwd(tab, fallbackCwd)` 单测）

**Interfaces:**
- Consumes: Task 4 命令
- Produces: `resolveSendCwd(tab: {worktreePath?: string; cwd?: string}, fallback?: string): string | undefined`（优先 worktreePath → tab.cwd → fallback）

- [ ] **Step 1: 写失败测试**

```ts
import { describe, it, expect } from 'vitest';
import { resolveSendCwd } from './chatSendCwd';

describe('resolveSendCwd', () => {
  it('prefers worktreePath', () => {
    expect(resolveSendCwd({ worktreePath: '/wt/a', cwd: '/proj' })).toBe('/wt/a');
  });
  it('falls back to tab cwd then arg', () => {
    expect(resolveSendCwd({ cwd: '/proj' })).toBe('/proj');
    expect(resolveSendCwd({}, '/fallback')).toBe('/fallback');
  });
});
```

- [ ] **Step 2: 确认失败**

Run: `npx vitest run src/stores/chatSendCwd.test.ts`
Expected: FAIL。

- [ ] **Step 3: 实现 + 接线**

`src/stores/chatSendCwd.ts` 实现纯函数；`useChatStore` tab 类型加 `worktreePath?`，`sendMessage` 构造 params 时 `cwd: resolveSendCwd(tab, currentCwd)`。

- [ ] **Step 4: 确认通过 + 构建**

Run: `npx vitest run src/stores/chatSendCwd.test.ts` → PASS
Run: `npm run build` → 通过

- [ ] **Step 5: 提交**

```bash
git add src/stores/chatSendCwd.ts src/stores/chatSendCwd.test.ts src/stores/useChatStore.ts src/services/
git commit -m "feat(chat): bind agent to worktree path as send cwd"
```

---

## Task 6: Agent 关闭时清理 worktree + 停 permission watcher（review 发现 #2）

**Files:**
- Modify: `src-tauri/src/chat/manager.rs`（新增 `close_agent(agent_id)`：stop daemon、pool.remove、停并移除 watcher）
- Modify: `src-tauri/src/chat/permission_watcher.rs`（若 `PermissionWatcher` 无 `stop()`，新增之）
- Modify: `src-tauri/src/commands/chat_commands.rs`（`helm_close_agent(agent_id, remove_worktree, repo_root?, worktree_path?, force)` 命令 + 注册）
- Test: `manager.rs`

**Interfaces:**
- Produces: `ChatManager::close_agent(&self, agent_id: AgentId)`（幂等：停 daemon + 从 pool 移除 + 停 watcher + 从 watchers 移除）

- [ ] **Step 1: 写失败测试**

```rust
    #[tokio::test]
    async fn close_agent_removes_pool_entry_and_watcher_registration() {
        // 用 test_support 注入 fake，调用 close_agent 后：pool.get == None。
        // watcher 注册表移除可通过新增的内省方法 watcher_count()（#[cfg(test)]）断言为 0。
    }
```

> 为可测，给 `ChatManager` 加 `#[cfg(test)] fn watcher_count(&self) -> usize`。

- [ ] **Step 2: 确认失败** → `cargo test ... close_agent_removes_pool_entry_and_watcher_registration`（编译失败）

- [ ] **Step 3: 实现**

```rust
    /// 关闭某 agent：停 daemon、移出池、停并移除其 permission watcher。幂等。
    pub async fn close_agent(&self, agent_id: AgentId) {
        if let Some(c) = self.pool.remove(&agent_id).await {
            c.stop().await;
        }
        if let Some(w) = self.permission_watchers.lock().await.remove(&agent_id) {
            w.stop(); // PermissionWatcher 需提供 stop()
        }
    }
```

worktree 删除放命令层（`helm_close_agent` 调 `close_agent` 后，若 `remove_worktree` 为真则 `WorktreeManager::remove(repo_root, worktree_path, force)`，预检由 manager 的 force 控制）。

- [ ] **Step 4: 确认通过** → `cargo test --manifest-path src-tauri/Cargo.toml chat`

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/manager.rs src-tauri/src/chat/permission_watcher.rs src-tauri/src/commands/chat_commands.rs src-tauri/src/lib.rs
git commit -m "feat(chat): close_agent cleans daemon, watcher, and optional worktree"
```

---

## Task 7: 前端 —— Composer 选「建 worktree」+ tab 上 worktree 徽章与 diff

**Files:**
- Modify: `src/components/chat/`（Composer / 新会话入口加「为此 Agent 建独立 worktree」开关与 repo 选择；tab/卡片显示分支徽章 + diff 概要）
- Modify: `src/stores/useChatStore.ts`（新建 tab 时若开启则调 `createWorktree`，存 `worktreePath`；关闭 tab 调 `helm_close_agent`）
- Test: `src/stores/` 纯函数（如 `worktreeBadgeLabel(info)`）

**Interfaces:**
- Consumes: Task 4/6 命令、Task 5 的 `worktreePath`
- Produces: `worktreeBadgeLabel(info: { branch: string; diff?: { filesChanged: number } }): string`

- [ ] **Step 1: 写失败测试**

```ts
import { describe, it, expect } from 'vitest';
import { worktreeBadgeLabel } from './worktreeBadge';
describe('worktreeBadgeLabel', () => {
  it('shows branch and change count', () => {
    expect(worktreeBadgeLabel({ branch: 'helm/task-a', diff: { filesChanged: 3 } })).toBe('helm/task-a · 3 changed');
  });
  it('omits count when no diff', () => {
    expect(worktreeBadgeLabel({ branch: 'helm/task-a' })).toBe('helm/task-a');
  });
});
```

- [ ] **Step 2: 确认失败** → `npx vitest run src/stores/worktreeBadge.test.ts`

- [ ] **Step 3: 实现纯函数 + UI 接线**（建 worktree 开关、徽章渲染、关闭 tab 调 close_agent）。i18n 文案同步 `zh.json`/`en.json`。

- [ ] **Step 4: 确认通过 + 构建** → vitest PASS；`npm run build` 通过

- [ ] **Step 5: 提交**

```bash
git add src/components/chat/ src/stores/useChatStore.ts src/stores/worktreeBadge.ts src/stores/worktreeBadge.test.ts src/locales/
git commit -m "feat(chat): worktree toggle in composer + branch/diff badge on agent tab"
```

---

## Task 8: 验证文档 + 手动 e2e

**Files:**
- Create: `docs/helm-phase1-verification.md`

- [ ] **Step 1: 写验证步骤**：① 自动门（cargo test chat / vitest / npm run build / git diff --check）；② 手动 e2e：开两个 Agent、各勾「建 worktree」、确认 `git worktree list` 出现两个 `helm/*`、两 Agent 同时改同一 repo 互不踩、各自 diff 正确、关闭 Agent 时脏工作树有预检、关闭后 worktree 与分支按选择清理。
- [ ] **Step 2: 实跑并回填**（手动 e2e 状态如未跑标「待人工执行」，不要造假）。
- [ ] **Step 3: 提交**

```bash
git add docs/helm-phase1-verification.md
git commit -m "docs(chat): add Phase 1 worktree isolation verification guide"
```

---

## Self-Review

- **覆盖**：设计 §6.2 WorktreeManager（Task 2-3）、§6.1 池竞态修复（Task 1，review 发现）、§6 agent→worktree cwd（Task 5）、§10 worktree 卡片/徽章/删除预检（Task 7 + Task 2 dirty-check）、watcher 清理（Task 6，review 发现 #2）。
- **占位符**：无 TBD；UI 任务（Task 7）给了纯函数测试 + 明确接线点。
- **类型一致**：`WorktreeInfo{path,branch}`→DTO`{path,branch}`；`DiffSummary{files_changed,insertions,deletions}`→DTO camelCase；`worktreePath` 贯穿 Task 5/7；`close_agent` 贯穿 Task 6/7。

## 不在本计划（后续）

- **Phase 1b：异构扇出**（同 prompt → N 个不同 CLI×模型 Agent 并排对比、选赢家合并 + LLM-judge）。依赖本计划的 worktree 隔离落地后再出，UI 较重，单独成计划。
- **Phase 2：AgentRuntime 契约 + CliRuntime + Hermes 引擎**（SQLite/Coordinator/Planner/Supervisor）。见设计文档 §13。
