# Helm × Hermes — Phase 1b：异构扇出（多 CLI×模型 并行 + 对比 + 选赢家）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Phase 1 的 worktree 隔离之上，实现"一个 prompt 扇出到 N 个异构 Agent（不同 CLI×模型，各自独立 worktree）并行执行 → 并排对比 + diff → 选赢家合并 / 丢弃其余"。这是 Helm 第一个"看得见的驾驶舱"能力。

**Architecture:** 扇出编排放前端（薄层：循环复用已有 `helm_worktree_create` + `chat_send(agentId)` 原语，给同组 tab 打 `fanoutGroupId`），**不预建 Phase 2 的编排引擎**（YAGNI）。后端只新增"合并赢家分支"能力（`WorktreeManager::merge_into_current` + `helm_worktree_merge` 命令）与一处 diff 口径修复。选兵 roster 复用 jadekit 现有 `useProviderStore` + `chatModels`。

**Tech Stack:** Rust（git via std::process::Command）、Tauri 2、React + TS + Zustand。

**前置依赖:** Phase 1 已合入（`WorktreeManager`、`helm_worktree_*`、`helm_close_agent`、`AgentPool`、前端 `worktreeService`/`worktreeBadge`/`chatSendCwd`/per-tab agentId）。本计划基于分支 `feat/helm-phase1-worktree`。

## Global Constraints

（沿用前几期）不写魔法字符串（fanout 状态/命令名/分支前缀集中常量）；JSON camelCase + Rust snake_case；新命令注册 `lib.rs` `generate_handler!`；worktree/git/daemon 操作经抽象；新增代码补中文注释；i18n 同步 `zh.json`/`en.json`；新增能力补测试 + 中文文档；每 task 结束 commit（Conventional Commits）；提交前 `cargo check` + 涉及前端 `npm run build`；`git diff --check` 干净。

**破坏性资源约束（强化）:** 合并（merge）与丢弃（discard worktree）都改写真实 git 状态。① 合并冲突时必须 `git merge --abort` 回到干净态，绝不把主仓库留在冲突中；② 丢弃 worktree 复用 `helm_close_agent(remove_worktree=true, force=true)`，但**必须前端二次确认**后才 force 删；③ 选赢家合并前若赢家 worktree 有未提交改动，提示用户（合并的是已提交内容）。

---

## 阅前必读

```bash
codegraph node WorktreeManager
codegraph node ChatManager
codegraph explore worktree merge fanout provider model chat_send close_agent
```
通读真实文件：
- `src-tauri/src/chat/worktree.rs`（`create`/`remove`/`list`/`diff_summary`/`has_uncommitted_changes`；本期加 `merge_into_current`，修 `diff_summary`）
- `src-tauri/src/commands/chat_commands.rs`（`helm_worktree_*`/`helm_close_agent` 现状，本期加 `helm_worktree_merge`）
- `src/stores/useChatStore.ts`（per-tab `agentId`/`worktreePath`/`sendMessage`/关闭 tab 调 `helm_close_agent`）
- `src/services/worktreeService.ts`（`createWorktree`/`removeWorktree`/`worktreeDiff`/`closeAgent`）
- `src/stores/useProviderStore.ts`（`get_providers` → `Provider[]`）、`src/utils/chatModels.ts`、`src/components/chat/composer/constants.ts`（`CLAUDE_MODELS`/`CODEX_MODELS`/`ChatProviderId`）—— 扇出选兵 roster 来源
- `src/components/chat/composer/ChatComposer.tsx`、`src/components/chat/ChatSessionTabs.tsx`（Phase 1 已接 worktree toggle / badge，本期加扇出入口与对比视图）

> 新建文件：前端 `src/stores/fanoutPlan.ts`(+test)、`src/components/chat/fanout/FanoutComposer.tsx`、`src/components/chat/fanout/FanoutCompareView.tsx`。后端就地 Modify `worktree.rs`/`chat_commands.rs`/`lib.rs`。

---

## Task 1: 修 `diff_summary` 口径——纳入未跟踪文件（Phase 1 review 遗留 #1）

现状 `git diff --shortstat HEAD` **不含未跟踪新文件**，导致"只新增文件"的 worktree 徽章显示 0 changed，但 `has_uncommitted_changes`（status）为真，口径不一致。

**Files:**
- Modify: `src-tauri/src/chat/worktree.rs`
- Test: 同文件

**Interfaces:**
- Produces: `diff_summary` 行为变更——新增未跟踪文件计入 `files_changed` 与 `insertions`（行数）。签名不变。

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn diff_summary_includes_untracked_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "task-u").unwrap();

        std::fs::write(info.path.join("brand_new.txt"), "a\nb\n").unwrap(); // 仅未跟踪新文件
        let s = WorktreeManager::diff_summary(&info.path).unwrap();
        assert!(s.files_changed >= 1, "未跟踪新文件应计入 files_changed");
        assert!(s.insertions >= 2, "未跟踪新文件行数应计入 insertions");
    }
```

- [ ] **Step 2: 确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree::tests::diff_summary_includes_untracked_new_file`
Expected: FAIL（files_changed=0）。

- [ ] **Step 3: 实现**

改 `diff_summary`：先把未跟踪文件用 `git add --intent-to-add`（`-N`）登记，使其进入 `git diff` 统计，再读 shortstat，最后还原（`git reset` 取消 intent-to-add，避免污染暂存区）：

```rust
    pub fn diff_summary(worktree_path: &Path) -> Result<DiffSummary, String> {
        // 把未跟踪文件登记为 intent-to-add，使其纳入 `git diff` 统计；读完即还原。
        let _ = Self::run(worktree_path, &["add", "--intent-to-add", "--", "."]);
        let out = Self::run(worktree_path, &["diff", "--shortstat", "HEAD"]);
        // 无论成功与否都还原 intent-to-add，避免污染暂存区。
        let _ = Self::run(worktree_path, &["reset", "--quiet"]);
        let out = out?;
        let mut summary = DiffSummary::default();
        for part in out.split(',') {
            let item = part.trim();
            let count: u32 = item.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            if item.contains("file") { summary.files_changed = count; }
            else if item.contains("insertion") { summary.insertions = count; }
            else if item.contains("deletion") { summary.deletions = count; }
        }
        Ok(summary)
    }
```

> 验证既有用例 `diff_summary_counts_changes_vs_head`（改已跟踪文件）仍通过。

- [ ] **Step 4: 确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree`
Expected: 全绿（含新用例 + 原有）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/worktree.rs
git commit -m "fix(chat): diff_summary counts untracked new files (intent-to-add)"
```

---

## Task 2: `WorktreeManager::merge_into_current`（合并赢家分支，冲突安全回滚）

**Files:**
- Modify: `src-tauri/src/chat/worktree.rs`
- Test: 同文件

**Interfaces:**
- Produces:
  - `pub enum MergeOutcome { Merged, Conflict }`
  - `WorktreeManager::merge_into_current(repo_root: &Path, source_branch: &str) -> Result<MergeOutcome, String>`：把 `source_branch` 合并进 `repo_root` 当前 HEAD 分支；冲突则 `git merge --abort` 回滚并返回 `Conflict`（不留冲突态）。

- [ ] **Step 1: 写失败测试**

```rust
    fn commit_file(dir: &Path, name: &str, content: &str, msg: &str) {
        std::fs::write(dir.join(name), content).unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-qm", msg]);
    }

    #[test]
    fn merge_clean_brings_worktree_commit_into_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let wts = tmp.path().join("worktrees");
        let info = WorktreeManager::create(&repo, &wts, "win").unwrap();

        commit_file(&info.path, "feature.txt", "done", "feat"); // 在 worktree 分支提交

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
        commit_file(&repo, "README.md", "from-main", "main change"); // 同文件冲突

        let outcome = WorktreeManager::merge_into_current(&repo, &info.branch).unwrap();
        assert!(matches!(outcome, super::MergeOutcome::Conflict));
        assert!(!WorktreeManager::has_uncommitted_changes(&repo).unwrap(), "冲突已 abort，主仓干净");
    }
```

- [ ] **Step 2: 确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree::tests::merge_clean_brings_worktree_commit_into_repo`
Expected: 编译失败（`merge_into_current`/`MergeOutcome` 未定义）。

- [ ] **Step 3: 实现**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged,
    Conflict,
}

impl WorktreeManager {
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
        // 失败：判断是否冲突；无论何种失败都尝试 abort 回到干净态。
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
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
```

- [ ] **Step 4: 确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::worktree`
Expected: 全绿。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/worktree.rs
git commit -m "feat(chat): WorktreeManager::merge_into_current with conflict-safe abort"
```

---

## Task 3: `helm_worktree_merge` 命令

**Files:**
- Modify: `src-tauri/src/commands/chat_commands.rs`、`src-tauri/src/lib.rs`
- Test: 编译级 + 复用 worktree 单测

**Interfaces:**
- Produces: `helm_worktree_merge(repo_root: String, source_branch: String, state) -> Result<MergeOutcomeDto, String>`；`MergeOutcomeDto`（serde：`{"outcome":"merged"|"conflict"}`，集中枚举映射，不写魔法串）

- [ ] **Step 1: 实现命令**

复用 `resolve_existing_chat_directory` + `resolve_git_repository`（chat_commands 已有）解析 repo_root，调 `WorktreeManager::merge_into_current`，把 `MergeOutcome` 映射成 DTO。

- [ ] **Step 2: 注册 + 编译**

`lib.rs` `generate_handler!` 加 `chat_commands::helm_worktree_merge`。
Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 3: 后端测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat`
Expected: PASS。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands/chat_commands.rs src-tauri/src/lib.rs
git commit -m "feat(chat): expose helm_worktree_merge command"
```

---

## Task 4: 前端扇出计划纯函数 `fanoutPlan`

把"prompt + N 个选兵 → N 个 Agent 描述"做成纯函数，便于测试与复用。

**Files:**
- Create: `src/stores/fanoutPlan.ts`、`src/stores/fanoutPlan.test.ts`

**Interfaces:**
- Produces:
  - `interface FanoutPick { providerId: string; chatProvider: 'claude' | 'codex'; model: string; }`
  - `interface FanoutAgentPlan { agentId: string; worktreeName: string; pick: FanoutPick; }`
  - `interface FanoutPlan { groupId: string; prompt: string; agents: FanoutAgentPlan[]; }`
  - `buildFanoutPlan(prompt: string, picks: FanoutPick[], makeId?: () => string): FanoutPlan`（每个 pick → 一个 agent；`worktreeName = fanout-<groupShort>-<index>-<model 简写>`，去非法字符；`agentId`/`groupId` 用 `makeId`，默认 `crypto.randomUUID`）

- [ ] **Step 1: 写失败测试**

```ts
import { describe, it, expect } from 'vitest';
import { buildFanoutPlan } from './fanoutPlan';

const picks = [
  { providerId: 'p1', chatProvider: 'claude' as const, model: 'claude-opus-4-8' },
  { providerId: 'p2', chatProvider: 'codex' as const, model: 'gpt-5-codex' },
];

describe('buildFanoutPlan', () => {
  it('creates one agent per pick with unique agentId and worktree name', () => {
    let n = 0;
    const plan = buildFanoutPlan('do X', picks, () => `id-${n++}`);
    expect(plan.agents).toHaveLength(2);
    expect(plan.prompt).toBe('do X');
    const ids = plan.agents.map((a) => a.agentId);
    expect(new Set(ids).size).toBe(2);
    const wts = plan.agents.map((a) => a.worktreeName);
    expect(new Set(wts).size).toBe(2);
    expect(wts.every((w) => /^[A-Za-z0-9._-]+$/.test(w))).toBe(true); // 路径安全
  });
  it('preserves pick provider/model on each agent', () => {
    const plan = buildFanoutPlan('x', picks, () => 'fixed');
    expect(plan.agents[0].pick.chatProvider).toBe('claude');
    expect(plan.agents[1].pick.model).toBe('gpt-5-codex');
  });
});
```

- [ ] **Step 2: 确认失败** → `npx vitest run src/stores/fanoutPlan.test.ts`（文件不存在）

- [ ] **Step 3: 实现** `src/stores/fanoutPlan.ts`：

```ts
// 扇出计划纯函数：prompt + 选兵 → N 个 Agent 描述（各自 worktree）。无副作用，便于测试。
export interface FanoutPick { providerId: string; chatProvider: 'claude' | 'codex'; model: string; }
export interface FanoutAgentPlan { agentId: string; worktreeName: string; pick: FanoutPick; }
export interface FanoutPlan { groupId: string; prompt: string; agents: FanoutAgentPlan[]; }

const safe = (s: string) => s.replace(/[^A-Za-z0-9._-]/g, '-').slice(0, 24);

export function buildFanoutPlan(
  prompt: string,
  picks: FanoutPick[],
  makeId: () => string = () => crypto.randomUUID(),
): FanoutPlan {
  const groupId = makeId();
  const groupShort = groupId.slice(0, 8);
  const agents = picks.map((pick, i) => ({
    agentId: makeId(),
    worktreeName: `fanout-${safe(groupShort)}-${i}-${safe(pick.model)}`,
    pick,
  }));
  return { groupId, prompt, agents };
}
```

- [ ] **Step 4: 确认通过** → `npx vitest run src/stores/fanoutPlan.test.ts` PASS

- [ ] **Step 5: 提交**

```bash
git add src/stores/fanoutPlan.ts src/stores/fanoutPlan.test.ts
git commit -m "feat(chat): add buildFanoutPlan pure function for fan-out planning"
```

---

## Task 5: store 扇出编排——建 N worktree + 同 prompt 群发，打 fanoutGroupId

**Files:**
- Modify: `src/stores/useChatStore.ts`（tab 增 `fanoutGroupId?: string`；新增 `launchFanout(repoRoot, plan)`：对每个 agent 调 `createWorktree` → 建 tab（绑 `agentId`/`worktreePath`/`fanoutGroupId`/model）→ `chat_send`；新增 `discardFanoutAgent(tab)` 调 `closeAgent(remove_worktree=true, force=true)`）
- Modify: `src/services/worktreeService.ts`（补 `mergeWorktree(repoRoot, sourceBranch)` 调 `helm_worktree_merge`）
- Test: `src/stores/` 纯函数 `fanoutTabsOf(tabs, groupId)`

**Interfaces:**
- Consumes: Task 3/4 + 既有 `createWorktree`/`closeAgent`/`chat_send`
- Produces: `fanoutTabsOf(tabs: {fanoutGroupId?: string}[], groupId: string): tabs[]`（筛同组 tab，供对比视图）

- [ ] **Step 1: 写失败测试**

```ts
import { describe, it, expect } from 'vitest';
import { fanoutTabsOf } from './fanoutGroup';
describe('fanoutTabsOf', () => {
  it('filters tabs by group id', () => {
    const tabs = [{ fanoutGroupId: 'g1' }, { fanoutGroupId: 'g2' }, { fanoutGroupId: 'g1' }];
    expect(fanoutTabsOf(tabs, 'g1')).toHaveLength(2);
  });
});
```

- [ ] **Step 2: 确认失败** → `npx vitest run src/stores/fanoutGroup.test.ts`

- [ ] **Step 3: 实现** `fanoutGroup.ts` 纯函数 + `useChatStore` 接 `launchFanout`/`discardFanoutAgent` + `worktreeService.mergeWorktree`。`launchFanout` 串起：每 agent `createWorktree(repoRoot, plan.agents[i].worktreeName)` → 存 `worktreePath`/`fanoutGroupId` → `sendMessage(prompt)`（cwd 走 Phase 1 的 `resolveSendCwd`）。

- [ ] **Step 4: 确认通过 + 构建** → vitest PASS；`npm run build` 通过

- [ ] **Step 5: 提交**

```bash
git add src/stores/useChatStore.ts src/stores/fanoutGroup.ts src/stores/fanoutGroup.test.ts src/services/worktreeService.ts
git commit -m "feat(chat): store orchestration for fan-out launch/discard/merge"
```

---

## Task 6: 扇出入口 UI（FanoutComposer：选兵 + 下达）

**Files:**
- Create: `src/components/chat/fanout/FanoutComposer.tsx`
- Modify: `src/components/chat/composer/ChatComposer.tsx`（加"扇出"模式开关，进入后渲染 FanoutComposer）
- Modify: `src/locales/zh.json`、`src/locales/en.json`（文案）
- Test: 纯函数 `rosterPicksFromProviders`（providers + 选择 → FanoutPick[]）

**Interfaces:**
- Consumes: `useProviderStore` 的 `Provider[]`、`chatModels`/`CLAUDE_MODELS`/`CODEX_MODELS`、Task 4 `FanoutPick`、Task 5 `launchFanout`
- Produces: `rosterPicksFromProviders(selected: {providerId: string; model: string}[], providers: Provider[]): FanoutPick[]`（按 provider 推断 `chatProvider`=claude/codex）

- [ ] **Step 1: 写失败测试**（`src/components/chat/fanout/roster.test.ts`）

```ts
import { describe, it, expect } from 'vitest';
import { rosterPicksFromProviders } from './roster';
const providers = [
  { id: 'p1', appType: 'claude' } as any,
  { id: 'p2', appType: 'codex' } as any,
];
describe('rosterPicksFromProviders', () => {
  it('maps selection to picks with inferred chatProvider', () => {
    const picks = rosterPicksFromProviders(
      [{ providerId: 'p1', model: 'claude-opus-4-8' }, { providerId: 'p2', model: 'gpt-5-codex' }],
      providers,
    );
    expect(picks).toEqual([
      { providerId: 'p1', chatProvider: 'claude', model: 'claude-opus-4-8' },
      { providerId: 'p2', chatProvider: 'codex', model: 'gpt-5-codex' },
    ]);
  });
});
```

> `chatProvider` 推断规则：按 provider 的应用类型字段（codegraph 确认 `Provider` 实际字段名，可能是 `appType`/`app_type`/`category`；以真实类型为准，测试同步真实字段）。

- [ ] **Step 2: 确认失败** → `npx vitest run src/components/chat/fanout/roster.test.ts`

- [ ] **Step 3: 实现** `roster.ts` 纯函数 + `FanoutComposer.tsx`（多选 provider×模型、输入 prompt、选 repo、点"扇出"调 `launchFanout`）+ ChatComposer 接入"扇出"模式 + i18n。

- [ ] **Step 4: 确认通过 + 构建** → vitest PASS；`npm run build` 通过

- [ ] **Step 5: 提交**

```bash
git add src/components/chat/fanout/ src/components/chat/composer/ChatComposer.tsx src/locales/
git commit -m "feat(chat): fan-out composer with provider/model roster selection"
```

---

## Task 7: 并排对比视图 + 选赢家 / 丢弃（FanoutCompareView）

**Files:**
- Create: `src/components/chat/fanout/FanoutCompareView.tsx`
- Modify: 主聊天布局（进入扇出组时渲染对比视图，复用现有 `ContentBlockRenderer`/`MessageItem` 渲染每个 agent 的会话；每栏显示 `CLI 图标 + 模型徽章 + AgentStateDot + diff 概要`）
- Modify: `src/locales/*`（"设为赢家/合并"、"丢弃"、合并冲突提示）
- Test: 纯函数 `winnerActionLabel(diff)` / `canMerge(tab)`

**Interfaces:**
- Consumes: Task 5 `fanoutTabsOf`/`discardFanoutAgent`/`mergeWorktree`、Phase 1 `worktreeBadge`/`worktreeDiff`、Phase 0 事件路由（`chat://stream` 按 agentId 已隔离到各栏）
- Produces: `canMerge(tab: {worktreePath?: string}): boolean`、`mergeConflictMessage(branch: string): string`

- [ ] **Step 1: 写失败测试**（`src/components/chat/fanout/compare.test.ts`）

```ts
import { describe, it, expect } from 'vitest';
import { canMerge } from './compare';
describe('canMerge', () => {
  it('requires a worktree path', () => {
    expect(canMerge({ worktreePath: '/wt/a' })).toBe(true);
    expect(canMerge({})).toBe(false);
  });
});
```

- [ ] **Step 2: 确认失败** → `npx vitest run src/components/chat/fanout/compare.test.ts`

- [ ] **Step 3: 实现** `compare.ts` 纯函数 + `FanoutCompareView.tsx`：N 栏并排；每栏底部"设为赢家（合并）"按钮 → 确认弹窗 → `mergeWorktree(repoRoot, tab.branch)`：
   - 返回 `merged` → 提示成功，询问是否丢弃其余（`discardFanoutAgent` 逐个，**force 前二次确认**）。
   - 返回 `conflict` → 用 `mergeConflictMessage` 提示"有冲突，已自动回滚，请手动处理该分支"，不丢弃任何东西。
   - "丢弃"按钮 → 二次确认 → `discardFanoutAgent`。

- [ ] **Step 4: 确认通过 + 构建** → vitest PASS；`npm run build` 通过

- [ ] **Step 5: 提交**

```bash
git add src/components/chat/fanout/ src/locales/
git commit -m "feat(chat): fan-out compare view with pick-winner merge and discard"
```

---

## Task 8（可选/可延后）：LLM-judge 推荐赢家

> **状态：可选 stretch。** 若想保持 Phase 1b 轻量，可跳过本 task，留到 Phase 2 与 Planner 一起做（judge 本质是 Planner 的一种）。做的话如下。

**Files:** Modify: `src/components/chat/fanout/FanoutCompareView.tsx`、新增 `src/stores/fanoutJudge.ts`(+test)

**Interfaces:** `buildJudgePrompt(prompt: string, candidates: {label: string; summary: string; diff: string}[]): string`（纯函数，拼一个让模型对比候选并给出推荐+理由的提示）

- [ ] Step 1-4：TDD `buildJudgePrompt` 纯函数（断言含每个候选 label/diff 与"只输出赢家 label + 理由"的约束）→ UI 加"AI 评判"按钮，复用 `chat_send` 起一个临时 judge agent（独立 agentId，无 worktree，cwd=repoRoot），把候选的 diff/摘要喂进去，解析推荐高亮。Step 5 commit `feat(chat): optional LLM-judge winner recommendation for fan-out`。

---

## Task 9: 验证文档 + 手动 e2e

**Files:** Create `docs/helm-phase1b-verification.md`

- [ ] Step 1: 写验证步骤：① 自动门（cargo test chat / vitest / npm run build / git diff --check）；② 手动 e2e：扇出选 2-3 个不同 CLI×模型 → 确认各自独立 worktree（`git worktree list` 出现多个 `helm/fanout-*`）、并排对比各栏输出**互不串流**、diff 概要各自正确 → 选一个赢家合并（`git log` 主分支出现该改动）→ 冲突场景验证主仓回滚干净 → 丢弃其余（worktree 与分支按确认清理）。
- [ ] Step 2: 实跑回填（未跑标「待人工执行」，不要造假）。
- [ ] Step 3: 提交 `docs(chat): add Phase 1b fan-out verification guide`。

---

## Self-Review

- **覆盖**：设计 §9 异构扇出/选赢家（Task 4-7）、LLM-judge（Task 8 可选）、§10 对比视图/状态点/徽章（Task 6-7）、merge 能力（Task 2-3）、Phase 1 遗留 #1 diff 口径（Task 1）。
- **占位符**：无 TBD；UI 任务均带纯函数测试 + 明确接线点 + codegraph 确认真实字段的指示。
- **类型一致**：`FanoutPick`/`FanoutAgentPlan`/`FanoutPlan`（Task 4）→ `launchFanout`/`fanoutTabsOf`（Task 5）→ `rosterPicksFromProviders`（Task 6）→ `canMerge`/`mergeWorktree`（Task 7）；`MergeOutcome`（Task 2）→ DTO（Task 3）→ 前端 `merged|conflict`（Task 7）一致。
- **破坏性安全**：merge 冲突 abort 回滚（Task 2 实测）；discard/force 删前端二次确认（Task 7）。

## 不在本计划（后续）

- **Phase 2：Hermes 引擎**（`AgentRuntime` 契约 + `CliRuntime` + SQLite Store + Coordinator 循环 + Planner 拆解/选兵/replan + WorkerSupervisor 判活）。扇出在 Phase 1b 是前端薄编排；Phase 2 把它升级为后端可持久化、可崩溃恢复、可 DAG 的真编排。见设计文档 §6/§13。
