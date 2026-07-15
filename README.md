<div align="center">
  <img src="public/logo.png" alt="JadeKit Logo" width="120" />

  # JadeKit

  **AI Agent Routing Kit**

  [![Website](https://img.shields.io/badge/🌐-Website-orange?logo=github-pages)](https://qincasin.github.io/jadekit/)
  [![Tauri](https://img.shields.io/badge/Tauri-v2-active?logo=tauri&color=FFC131)](#)
  [![React 19](https://img.shields.io/badge/React-19-blue?logo=react)](#)
  [![Rust](https://img.shields.io/badge/Rust-Backend-orange?logo=rust)](#)
  [![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

  [简体中文](#简体中文) | [English](#english)

</div>

---

<span id="简体中文"></span>

## 简介

**JadeKit** 是一个跨平台桌面应用，为 AI CLI 与 Agent 工具链（Claude Code、Codex CLI、Gemini CLI 等）提供统一的路由、配置管理与增强体验。

无论你是独立开发者还是多项目维护者，JadeKit 让你：
- 一键切换 API Key 和服务商，告别手动编辑环境变量
- 管理多个 Antigravity（Google Cloud Code）账号，自动刷新 Token 和监控配额
- 通过内置本地代理透明转发和加速 API 请求
- 在图形界面中管理 MCP 服务器、Prompt 预设、技能和子代理
- 统一查看多个 AI 工具的用量统计和会话记录

## 核心特性

### 🔑 多服务商管理
- 表格/卡片双视图展示，支持搜索、排序、拖拽排序
- 一键切换活跃 Provider，自动写入对应工具的配置文件
- 支持 Deep Link 一键导入 Provider（`jadekit://` 和 `ccswitch://`）

### 🛡️ Antigravity 账号管理
- **多账号统一管理**：支持添加、切换、禁用/启用多个 Antigravity（Google Cloud Code）账号
- **OAuth 浏览器登录**：集成 Google OAuth2 流程，一键授权添加新账号
- **Token 自动刷新**：自动检测 Token 过期并刷新，保持连接可用
- **配额实时监控**：查看每个账号的模型配额余量（百分比），配额低于阈值自动告警
- **订阅层级识别**：自动识别 FREE / PRO / ULTRA 订阅等级，按层级筛选账号
- **账号预热**：一键预热账号，激活 Token 确保可用性
- **批量操作**：批量删除、批量刷新配额、批量预热
- **一键导入**：从已有 Manager 配置中批量导入账号
- **操作日志**：记录所有账号操作历史，便于审计追踪
- **卡片/列表双视图**：灵活切换展示模式

### 🌐 内置本地代理服务器
- 基于 Rust + Axum 构建的高性能 HTTP 反向代理
- 自动健康检查 + 熔断器 + 故障切换（Failover）
- Thinking Budget 修正、Model 映射、请求/响应日志
- 可视化代理状态与请求监控

### 🧩 MCP 服务器集成
- 图形化添加、编辑、删除 MCP 服务器配置
- 支持全局级和项目级 MCP 配置
- 一键同步到 Claude、Codex、Gemini 等多个工具

### 🔮 Prompt / 技能 / 子代理管理
- Prompt 预设管理：创建、编辑、分发到指定工具的配置文件
- 技能（Skills）管理：支持发现、安装、卸载、跨应用同步
- 子代理（Subagents）管理：自定义子代理模板的 CRUD

### 📊 仪表盘与用量统计
- 统计概览：项目数、会话数、Token 消耗趋势
- 活跃度图表：按天统计的会话活跃度
- 多工具统一会话管理，支持在终端中恢复会话

### 🔧 其他
- 🎨 暗黑模式 + 系统跟随（View Transition 圆形扩散动画）
- 🌍 中英文国际化（i18next）
- 🔄 自动更新检查 + 后台静默下载
- ☁️ WebDAV 备份 / 本地自动备份 / 一键恢复
- 🖥️ 系统托盘常驻，关闭窗口不退出
- 📋 Deep Link 导入，支持从浏览器一键添加配置
- 🚀 开机自启动（可选）

## 截图

> 截图可补充在 `docs/` 或 `website/` 中

## 安装

前往 [GitHub Releases](https://github.com/qincasin/jadekit/releases) 下载适合你系统的安装包：

| 平台 | 格式 | 说明 |
|------|------|------|
| **Windows** | `.exe` / `.msi` | NSIS 安装向导或 MSI |
| **macOS** | `.dmg` | 支持 Apple Silicon & Intel |
| **Linux** | `.deb` / `.AppImage` | Debian 系或通用格式 |

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | React 19 · TypeScript 5.8 · Vite 7 |
| UI | TailwindCSS 3 · DaisyUI 4 · Lucide Icons |
| 状态 | Zustand 5 |
| 国际化 | i18next (zh/en) |
| 路由 | react-router-dom 7 (HashRouter) |
| 后端 | Rust (edition 2021) · Tauri 2 |
| 网络 | Axum 0.8 · Hyper 1 · Tokio · Reqwest 0.12 |
| 存储 | SQLite (rusqlite) · serde + serde_json |
| 其他 | arboard (剪贴板) · sysinfo · chrono · WebDAV |

## 项目结构

```
src/                              # React 前端
├── App.tsx                       # 路由定义 + 全局初始化
├── pages/                        # 页面组件
│   ├── Dashboard.tsx             # 统计概览、活跃度、项目列表
│   ├── ClaudePage.tsx            # API Token 管理
│   ├── AntigravityPage.tsx       # Antigravity 账号管理
│   ├── ProvidersPage.tsx         # 多服务商统一管理
│   ├── ProxyPage.tsx             # 本地代理配置与监控
│   ├── McpPage.tsx               # MCP 服务器管理
│   ├── PromptsPage.tsx           # Prompt 预设管理
│   ├── SkillsPage.tsx            # 技能管理
│   ├── SubagentsPage.tsx         # 子代理管理
│   ├── WorkspacesPage.tsx        # 工作区管理
│   ├── UsagePage.tsx             # 用量统计
│   └── Settings.tsx              # 设置（主题、语言、备份、更新）
├── components/                   # 组件
│   ├── layout/                   # 布局 (Layout, Navbar)
│   ├── common/                   # 通用组件 (ModalDialog, Toast)
│   ├── providers/                # Provider 相关组件
│   ├── proxy/                    # 代理相关组件
│   ├── mcp/                      # MCP 相关组件
│   ├── settings/                 # 设置相关组件
│   ├── dashboard/                # 仪表盘组件
│   ├── antigravity/              # Antigravity 账号组件
│   └── usage/                    # 用量组件
├── stores/                       # Zustand 状态管理
├── locales/                      # i18n 翻译 (zh.json / en.json)
└── types/                        # TypeScript 类型定义

src-tauri/src/                    # Rust 后端
├── lib.rs                        # Tauri 命令注册 + 应用初始化
├── main.rs                       # 入口
├── commands/                     # Tauri 命令层（按功能拆分）
│   ├── provider_commands.rs      # Provider CRUD + 健康检查
│   ├── proxy_commands.rs         # 代理启停 + 状态查询
│   ├── mcp_commands.rs           # MCP 服务器管理 (v2 数据库版)
│   ├── prompt_commands.rs        # Prompt 管理 + 同步
│   ├── skill_commands.rs         # 技能发现/安装/同步
│   ├── session_commands.rs       # 会话查询
│   ├── backup_commands.rs        # 数据库备份/恢复
│   ├── advanced_commands.rs      # WebDAV、自启动、用量
│   ├── antigravity_commands.rs   # Antigravity 账号管理
│   ├── deeplink_commands.rs      # Deep Link 导入
│   └── utility_commands.rs       # 导入导出、环境检测
├── models/                       # 数据模型 (serde 序列化)
│   ├── token.rs                  # ApiToken
│   ├── provider.rs               # Provider
│   ├── config.rs                 # 应用配置
│   ├── mcp.rs                    # MCP 服务器
│   ├── prompt.rs                 # Prompt 预设
│   ├── skill.rs                  # 技能
│   ├── subagent.rs               # 子代理
│   ├── proxy.rs                  # 代理配置
│   ├── usage.rs                  # 用量统计
│   └── antigravity.rs            # Antigravity 账号 + 配额 + 操作日志
├── services/                     # 业务逻辑层
│   ├── proxy_service.rs          # 代理生命周期管理
│   ├── provider_service.rs       # Provider 配置读写
│   ├── token_service.rs          # Token CRUD + 切换
│   ├── antigravity_service.rs    # Antigravity OAuth / Token / 配额 / 预热
│   ├── mcp_service.rs            # MCP 配置同步
│   ├── prompt_service.rs / _v2   # Prompt 管理 (文件 + 数据库)
│   ├── skill_service.rs / _v2    # 技能管理
│   ├── dashboard_service.rs      # 仪表盘数据聚合
│   ├── usage_service.rs          # 用量统计
│   ├── migration_service.rs      # 数据迁移 (JSON → SQLite)
│   ├── updater_service.rs        # 自动更新
│   ├── webdav_service.rs         # WebDAV 远程备份
│   └── ...
├── proxy/                        # 内置代理服务器核心
│   ├── server.rs                 # Axum 服务器启动
│   ├── handlers.rs               # 请求处理器
│   ├── health.rs                 # 健康检查
│   ├── circuit_breaker.rs        # 熔断器
│   ├── failover_switch.rs        # 故障切换
│   ├── model_mapper.rs           # 模型名称映射
│   ├── thinking_rectifier.rs     # Thinking Budget 修正
│   ├── provider_router.rs        # Provider 路由分发
│   ├── providers/                # Provider 适配器
│   └── usage/                    # 用量追踪
├── mcp/                          # MCP 多工具适配
│   ├── claude.rs                 # Claude MCP 配置
│   ├── codex.rs                  # Codex MCP 配置
│   ├── gemini.rs                 # Gemini MCP 配置
│   └── validation.rs             # 配置校验
├── database/                     # SQLite 数据库层
├── deeplink/                     # Deep Link 解析
├── session_manager/              # 会话管理器
├── tray.rs                       # 系统托盘
└── store.rs                      # Tauri 状态管理 (AppState)
```

## 开发

### 环境依赖

- [Node.js](https://nodejs.org/) v20+
- [Rust](https://www.rust-lang.org/tools/install) (edition 2021)
- [Tauri CLI v2](https://v2.tauri.app/start/prerequisites/)

### 本地运行

```bash
# 克隆仓库
git clone https://github.com/qincasin/jadekit.git
cd jadekit

# 安装前端依赖
npm install

# 启动开发模式 (Vite HMR + Tauri 后端)
npm run tauri dev
```

### 本地验证

提交改动前建议执行以下前端验证：

```bash
npm run lint       # 检查 TypeScript / React 代码规范
npm run test:run   # 运行前端单元测试
npm run build      # 执行类型检查并构建前端产物
```

如果改动涉及 Rust、Tauri command、后端服务、代理、MCP、会话或 SQLite，还需要额外执行：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
```

Rust 测试使用单线程运行是为了避免已知的 PTY 并发竞态。

### 生产构建

```bash
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。

### 版本管理

```bash
npm run bump patch "修复描述"    # z: 1.0.0 → 1.0.1
npm run bump minor "新增功能"    # y: 1.0.0 → 1.1.0
npm run bump major "重大更新"    # x: 1.0.0 → 2.0.0
```

脚本会自动同步更新 `package.json`、`Cargo.toml`、`tauri.conf.json` 等文件中的版本号。详见 [docs/versioning.md](docs/versioning.md)。

## 数据路径

| 文件 | 路径 | 说明 |
|------|------|------|
| 应用数据库 | `~/.jadekit/jadekit.db` | SQLite 主数据存储 |
| 应用配置 | `~/.jadekit/config.json` | 主题、语言等设置 |
| Token 数据 | `~/.ci/claude_switch.json` | API Key 列表（兼容旧版） |
| Claude 设置 | `~/.claude/settings.json` | Claude Code 运行时配置 |
| MCP 全局配置 | `~/.claude.json` | MCP 服务器定义 |
| 项目数据 | `~/.claude/projects/` | 各项目会话记录 |
| 历史记录 | `~/.claude/history.jsonl` | 命令历史 |

## 许可证

[MIT License](LICENSE) © JadeKit Contributors

---

<span id="english"></span>

## Overview

**JadeKit** is a cross-platform desktop app that provides unified configuration management for AI CLI tools like Claude Code, Codex CLI, and Gemini CLI. It offers a seamless GUI to manage API keys, Antigravity accounts, local proxies, MCP servers, prompts, skills, subagents, and usage stats — without ever touching dotfiles.

### Highlights

- **Multi-Provider Management** — One-click switching between API providers (official, Azure, third-party proxies) with table/card dual views and drag-and-drop reordering
- **Antigravity Account Management** — Manage multiple Google Cloud Code (Antigravity) accounts with OAuth browser login, automatic token refresh, real-time quota monitoring, tier detection (FREE/PRO/ULTRA), account warmup, batch operations, and operation logs
- **Built-in Rust Proxy** — High-performance local reverse proxy (Axum + Hyper) with health checks, circuit breaker, failover, model mapping, and thinking budget rectification
- **MCP Integration** — Visual MCP server management with cross-tool sync (Claude / Codex / Gemini)
- **Prompt / Skill / Subagent** — Full CRUD with workspace-level distribution and cross-app sync
- **Dashboard & Usage** — Unified session tracking, token consumption trends, and activity history across tools
- **Deep Link Import** — Import provider configs from browsers via `jadekit://` or `ccswitch://` URLs
- **Auto-Update & Backup** — Background update checks with silent download; WebDAV + local automatic backup
- **System Tray** — Minimize to tray; re-open from dock (macOS) or taskbar
- **Dark Mode** — System-aware dark mode with View Transition circular reveal animation
- **i18n** — Chinese and English with i18next

### Download

Get the latest release from [GitHub Releases](https://github.com/qincasin/jadekit/releases):

- **Windows**: `.exe` (NSIS) or `.msi`
- **macOS**: `.dmg` (Universal: Apple Silicon + Intel)
- **Linux**: `.deb` or `.AppImage`

### Tech Stack

**Frontend:** React 19 · TypeScript 5.8 · Vite 7 · TailwindCSS 3 · DaisyUI 4 · Zustand 5 · i18next

**Backend:** Rust · Tauri 2 · Axum 0.8 · Hyper 1 · Tokio · SQLite (rusqlite) · serde

### Development

```bash
git clone https://github.com/qincasin/jadekit.git
cd jadekit
npm install
npm run tauri dev      # Development
npm run tauri build    # Production build
```

#### Local validation

Before submitting changes, run the frontend validation commands:

```bash
npm run lint       # Check TypeScript / React code quality
npm run test:run   # Run frontend unit tests
npm run build      # Type-check and build the frontend
```

When changes touch Rust, Tauri commands, backend services, proxy code, MCP, sessions, or SQLite, also run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
```

Rust tests run with one test thread to avoid the known PTY concurrency race.

### License

[MIT](LICENSE) © JadeKit Contributors

---

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=qincasin/jadekit&type=Date)](https://star-history.com/#qincasin/jadekit&Date)
