# Agent 问答聊天功能 — 功能清单与架构说明

> 本文档说明 JadeKit 内置的「Agent 问答聊天」功能：它能做什么、如何实现、关键路径在哪。
> 面向维护者，便于后续在此基础上扩展（如模型列表动态化、供应商级长上下文等）。

## 1. 功能概述

一个内置于桌面应用的交互式 AI 编码助手（Claude Code / Codex 的图形化形态）。
用户在选定的工作目录（workspace）中与 Claude（或 Codex）进行多轮对话，AI 可读写文件、
执行命令、调用 MCP 工具、派生子代理（subagent），并通过权限弹窗征求用户许可。

整体分三层：

```
React 前端 (ChatPage + useChatStore)
   │  Tauri invoke / event
Rust 后端 (chat::ChatManager → DaemonClient)
   │  NDJSON over stdin/stdout
ai-bridge Node 守护进程 (daemon.js + @anthropic-ai/claude-agent-sdk)
   │  HTTPS
Anthropic / 第三方 Provider API
```

**核心机制**：agentic 的"大脑"（工具编排、文件/命令执行、子代理、MCP 集成）由官方
Claude Agent SDK / Codex SDK 提供；JadeKit 负责图形外壳、进程与协议桥接、权限交互、
流式渲染。两套异构 SDK 通过 ai-bridge 统一成同一套 NDJSON 协议，前端切换 provider 即可。

## 2. 功能清单

### 2.1 会话与对话
- 多轮流式对话，按 token 实时增量渲染（`[CONTENT_DELTA]` / `[CONTENT]`）。
- 多标签会话（Session Tabs），可并行多个工作目录会话、切换、关闭。
- 会话侧边栏（ChatSessionSidebar）：按项目分组、置顶/归档/已读未读、重命名、删除、折叠。
- 历史会话恢复（resume）：加载历史消息窗口，按需展开完整历史。
- 新建会话、选择工作目录（cwd）、跨会话切换 Provider。

### 2.2 输入与补全（composer）
- 富文本输入框（contentEditable），支持粘贴图片附件（base64）。
- `@` 文件引用补全（扫描工作目录文件）。
- `/` Slash 命令补全（内置 + 用户 + 项目级命令）。
- Prompt 增强器（PromptEnhancerDialog，调用 ai-bridge 的 prompt-enhancer）。
- 模型选择、permission mode 切换、长上下文（1M）开关、reasoning effort。
- Token 用量指示器（TokenIndicator）、上下文条（ContextBar）。

### 2.3 消息渲染
- Markdown 渲染（marked + highlight.js + dompurify）。
- 内容块渲染：text / image / thinking / tool_use / tool_result。
- 工具块（toolBlocks）：Bash、Edit（diff 预览）、Read、Search、Generic，及分组聚合。
- 子代理（Task）执行块：实时展示子代理消息流、历史回放（SubagentHistoryPanel）。
- 思考块（ThinkingBlock）、消息锚点导航（MessageAnchorRail）、会话内搜索。

### 2.4 权限与交互
- 工具权限弹窗（ToolPermissionDialog）。
- 计划审批弹窗（PlanApprovalDialog，plan mode）。
- AskUserQuestion 弹窗（AI 主动向用户提问）。
- 权限通过文件 watcher（permission_watcher）落盘 IPC 与 daemon 通信。

### 2.5 运行时与依赖管理
- 懒启动 ai-bridge daemon（首次发送时启动）；心跳保活、崩溃重连。
- Node 运行时检测（系统 node 优先，版本 ≥18 即用；不足时下载私有 Node 兜底）。
- SDK 安装/卸载（sdk_installer.rs，安装 `@anthropic-ai/claude-agent-sdk` 到数据目录）。
- 依赖状态面板（SdkDependencyPanel）、Node 版本校验。
- 调试模式（DebugModePanel + 守护进程日志 daemonLogs）。

### 2.6 工作区集成
- Git 状态检测、分支列表、新建并切换分支。
- 在终端打开项目 / resume 会话、在文件管理器打开路径。
- diff 审查面板（ChatDiffReviewPane）、在外部编辑器打开文件（editor_commands）。
- 桌面系统通知（任务完成提醒）。

## 3. 架构与实现

### 3.1 前端 (`src/`)
| 文件 | 职责 |
|------|------|
| `pages/ChatPage.tsx` | 页面装配：侧边栏 + 消息区 + composer + 各类弹窗 |
| `stores/useChatStore.ts` | 核心状态机：会话/标签、发送、流式解析、事件监听、权限 |
| `stores/useSdkStore.ts` | SDK/Node 运行时依赖状态 |
| `components/chat/**` | 消息列表、composer、弹窗、侧边栏、状态面板 |
| `components/toolBlocks/**` | 各类工具调用的可视化块与分组 |
| `utils/chat*.ts` | 状态摘要、模型、导航、MCP 连通性、消息流、布局等纯函数 |
| `types/chat.ts, session.ts, tools.ts, toolblock.ts, permission.ts` | 类型定义 |

**store 关键 action**：`init`（注册所有 `chat://*`、`permission://*` 事件监听）、
`send`、`abort`、`loadSession`、`startNewSession`、`setProvider/Model/PermissionMode`、
`answerToolPermission`、`reconnectDaemon`。

**流式协议解析**（store 内 `chat://stream` 监听）按前缀标签分发：
`[SESSION_ID]`/`[THREAD_ID]`、`[CONTENT_DELTA]`、`[CONTENT]`、`[USAGE]`、`[BLOCK_RESET]`。
结构化消息走 `chat://message`，子代理消息走 `chat://subagent-message`。

### 3.2 Rust 后端 (`src-tauri/src/`)
| 模块 | 职责 |
|------|------|
| `chat/mod.rs` | 模块入口，导出 ChatManager 等 |
| `chat/manager.rs` | 高层编排：懒启动 daemon、转发流/生命周期事件、心跳循环 |
| `chat/daemon_client.rs` | 进程管理：spawn node、NDJSON 读写、按 request id 多路复用 |
| `chat/protocol.rs` | NDJSON 协议类型（`DaemonRequest` / `RawLine` / `StreamLine` / `DaemonEvent`） |
| `chat/node_runtime.rs` | Node 运行时检测/下载（带 sha256 校验） |
| `chat/sdk_installer.rs` | claude-agent-sdk / codex-sdk 安装/卸载/状态/版本查询 |
| `chat/permission_watcher.rs` | 权限 IPC 文件 watcher，写回许可响应 |
| `chat/resources.rs` | 资源路径解析（数据目录、ai-bridge、permission_dir） |
| `chat/slash_commands.rs` | 列举 slash 命令 |
| `commands/chat_commands.rs` | Tauri 命令层：send/abort/daemon/sdk/workspace/git/enhance/permission |
| `commands/session_commands.rs` | 统一会话历史读取、project/session 管理 |
| `commands/editor_commands.rs` | 在外部编辑器打开文件 |
| `models/chat.rs` | `ChatMessageEvent` / `SubagentMessageEvent` 事件载荷 |
| `session_manager/` | 跨 provider 会话枚举与历史读取（含 workspace_metadata） |

**前端事件通道**：`chat://stream`、`chat://done`、`chat://message`、`chat://subagent-message`、
`chat://daemon`、`permission://ask-user-question`、`permission://plan-approval`、`permission://tool`。

**注册点**：`lib.rs` 中 `mod chat;`、`use commands::{chat_commands, session_commands, editor_commands}`、
`generate_handler!` 注册约 30 个命令、`app.manage(ChatState{...})`、退出时 `chat_state.manager.shutdown()`。

### 3.3 ai-bridge Node 守护进程 (`src-tauri/resources/ai-bridge/`)
- `daemon.js`：NDJSON 主循环，按 method 路由到 channel。
- `channels/claude-channel.js`、`channels/codex-channel.js`、`channel-manager.js`。
- `services/claude/**`：消息发送、流事件处理、session、MCP 状态探测、权限模式。
- `services/codex/**`：Codex 适配。
- `utils/**`：sdk-loader、claude-cli-path、model-utils、permission-mapper 等。
- 依赖 `@anthropic-ai/claude-agent-sdk` / `@openai/codex-sdk`（运行时安装到数据目录，非打包）。

## 4. 数据目录与运行时依赖

- **应用数据目录**：统一走 `services::app_paths`（`~/.jadekit/`）。
  - SDK 安装目录：`~/.jadekit/ai-bridge-deps/dependencies/<sdkId>/node_modules/<pkg>`
    （ai-bridge 通过 `AI_BRIDGE_DEPS_DIR` 环境变量定位，由 Rust 注入）。
  - 私有 Node 运行时：`~/.jadekit/runtime/node/`。
  - 权限 IPC 目录：`<app-data-dir>/permissions/`。
- **Node 运行时**：优先复用系统 node（≥18），不足时下载私有 Node（下载后做 sha256 校验）。
- **SDK**：首次按需 `npm install` 到上述目录；版本默认锁定 caret 区间（claude-agent-sdk `^0.2.58`），
  不自动追最新；面板可查 registry 最新版并手动升级到指定版本。SDK 不走全局 node_modules，
  仅从固定 deps 目录加载，以保证版本确定性与行为一致性。

## 5. Provider / 模型映射机制

聊天 daemon 启动时，从数据库取**当前激活的 claude provider** 的 `apiKey` + `url`，
作为 `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_BASE_URL` 传给 daemon。模型映射通过 provider 的
`defaultSonnetModel` / `defaultOpusModel` / `defaultHaikuModel` / `defaultReasoningModel` 字段配置
（如 GLM provider 可全部填 `glm-5.2`），SDK 内部按 sonnet/opus/haiku 槽位调用时会被翻译成真实模型。

> **待扩展**：当前聊天页模型列表以静态 Claude/Codex 模型表为兜底，叠加 provider 配置的模型。
> 计划改为"跟随实际生效配置"——优先读 `~/.claude/settings.json` 的模型字段，未配置时动态拉取
> 当前 base_url 的 `/v1/models`。此外计划**新增**供应商（provider）级的长上下文能力配置
> （标记/配置该 provider 是否支持长上下文及其上限），**聊天页现有的 1M 运行时开关保持不变**。
