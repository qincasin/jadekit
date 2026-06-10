# 工作区管理

## 功能概述

Workspaces 页面提供项目工作区浏览、会话查看和终端快速启动功能。数据来源于 `~/.claude/projects/` 目录下的真实项目记录，无需手动配置。

## 页面路由

`/workspaces` → `src/pages/WorkspacesPage.tsx`

## 核心能力

### 项目列表

- 自动扫描 `~/.claude/projects/` 下所有项目目录
- 显示项目名称、路径、会话数量、最近活跃时间
- 支持搜索过滤

### 会话查看

- 点击项目展开该项目的会话列表
- 显示会话元数据（Provider、时间、消息数等）
- 查看会话的完整消息记录
- 多 Provider 统一会话（Claude / Codex / Gemini 合并展示）

### 终端快速启动

通过 `open_in_terminal` Tauri 命令在指定目录打开终端。

**支持平台与终端：**

| 平台 | 终端 | 优先级 |
|------|------|--------|
| macOS | Ghostty → iTerm2 → cmux → Warp → Terminal | 自动检测已安装的终端 |
| Windows | WT → PowerShell → CMD | 可配置 |
| Linux | GNOME Terminal → Konsole → xterm | 自动检测 |

终端偏好可在 Settings 中配置（`config.preferred_terminal`）。

### 会话恢复

`launch_resume_session` 命令可在指定工作目录中恢复历史会话，使用与快速启动相同的终端检测逻辑。

## 相关代码

| 文件 | 说明 |
|------|------|
| `src/pages/WorkspacesPage.tsx` | 前端页面 |
| `src/types/session.ts` | 会话类型定义 |
| `src-tauri/src/commands/session_commands.rs` | 会话查询命令 |
| `src-tauri/src/lib.rs` → `open_in_terminal` | 终端打开（含平台检测） |
| `src-tauri/src/lib.rs` → `launch_resume_session` | 会话恢复 |
| `src-tauri/src/services/dashboard_service.rs` | 项目列表数据 |
