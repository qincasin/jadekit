# Codex 配置集成

## 功能概述

JadeKit 通过 MCP 多工具适配层管理 Codex CLI 的配置，支持将 MCP 服务器配置同步到 Codex 的 TOML 格式配置文件中。

## 配置文件

| 文件 | 路径 | 格式 |
|------|------|------|
| Codex 配置 | `~/.codex/config.toml` | TOML |
| Codex 认证 | `~/.codex/auth.json` | JSON |

## MCP 配置同步

Codex 的 MCP 服务器配置存储在 `config.toml` 中（TOML 格式），与 Claude 的 JSON 格式不同。`src-tauri/src/mcp/codex.rs` 负责格式转换：

- 自动检测 Codex 是否安装（`~/.codex` 目录是否存在）
- 将 JadeKit 内部的 MCP 配置转换为 Codex TOML 格式
- 支持 stdio / http / sse 三种 MCP 服务器类型
- 读取 Codex 已有的 MCP 配置导入到 JadeKit

### TOML ↔ JSON 转换

```
JadeKit (JSON)                    Codex (TOML)
─────────────                    ────────────
{ "command": "npx",              [mcp-server-name]
  "args": ["-y", "foo"],    →    type = "stdio"
  "env": {"KEY": "VAL"} }        command = "npx"
                                 args = ["-y", "foo"]
                                 env.KEY = "VAL"
```

## Provider 集成

Codex 的 API 认证通过 Provider 系统统一管理。切换 Provider 时，JadeKit 将 API Key 和相关配置写入 `~/.codex/auth.json`。

## 相关代码

| 文件 | 说明 |
|------|------|
| `src-tauri/src/mcp/codex.rs` | Codex MCP 配置读写与格式转换 |
| `src-tauri/src/services/provider_service.rs` | Provider 统一管理（含 Codex） |
| `src-tauri/src/models/app_type.rs` | 应用类型枚举（Claude/Codex/Gemini） |
