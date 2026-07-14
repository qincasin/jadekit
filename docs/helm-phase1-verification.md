# Helm × Hermes — Phase 1 验证（Worktree 隔离）

> Phase 1 在 Phase 0 的 per-agent daemon pool 之上增加每 Agent 独立 git worktree，
> 并修复两个 Phase 0 review 发现的问题：AgentPool 并发 init 败者 daemon 回收、
> Agent 关闭时 permission watcher 清理。

## 1. 自动化检查

| 检查 | 命令 | 结果 |
|------|------|------|
| Rust 编译 | `cargo check --manifest-path src-tauri/Cargo.toml` | 通过：`Finished dev profile` |
| Rust chat 测试 | `cargo test --manifest-path src-tauri/Cargo.toml chat` | 通过：66 passed; 0 failed |
| 前端 worktree 单测 | `npx vitest run src/stores/chatSendCwd.test.ts src/stores/worktreeBadge.test.ts` | 通过：2 files, 4 tests |
| 前端构建 | `npm run build` | 通过：`tsc && vite build`，Vite build completed |
| 空白规范 | `git diff --check` | 待最终提交前执行 |

## 2. 覆盖点

- `AgentPool::get_or_init` 并发双检败者会调用 `stop()`，避免 orphan daemon。
- `WorktreeManager` 覆盖 create / list / remove / dirty-check / diff summary。
- Tauri 命令暴露 `helm_worktree_create/remove/list/diff` 与 `helm_close_agent`。
- 前端发送 cwd 优先级为 `worktreePath -> tab cwd -> fallback cwd`。
- Composer 可为当前 Agent 创建独立 worktree；Agent tab 显示分支与 diff 文件数。
- 关闭 tab 会调用 `helm_close_agent` 清理 daemon 与 permission watcher；不会自动删除 worktree。

## 3. 手动 e2e

状态：待人工执行。

建议步骤：

1. 启动应用：`npm run tauri dev`。
2. 进入 Helm / Chat 页面，选择一个 Git 仓库工作目录。
3. 开两个 Agent tab，分别勾选“为此 Agent 建独立 worktree”。
4. 分别发送会修改同一文件的任务。
5. 另开终端执行 `git worktree list`，确认出现两个 `helm/*` worktree。
6. 确认两个 Agent 的 cwd 分别为自己的 worktree，互不覆盖同一 checkout。
7. 确认 tab 上显示 worktree 分支徽章与 diff 文件数。
8. 关闭 Agent tab，确认 daemon 与 permission watcher 被关闭。
9. 若后续执行删除 worktree 操作，先验证脏工作树非 force 会拒删，绝不强删用户未确认的工作树。

## 4. 已知偏差

- 当前 Phase 1 只做单 Agent worktree 绑定与基础 UI；未实现 Phase 1b 的异构扇出 / 赢家合并。
- 关闭 tab 只关闭 agent daemon 与 watcher，不自动删除 worktree，避免未确认的破坏性删除。
- 手动 e2e 尚未在桌面应用中执行，状态如上标记为待人工执行。
