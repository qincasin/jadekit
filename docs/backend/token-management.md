# Claude API Token 管理系统

> **注意**: Token 管理已作为 Provider 系统的兼容层保留。新功能请使用 Provider 系统（`provider_service.rs`）。

## 1. 功能概述

管理多个 Claude API Token，支持快速切换。切换时将选中 Token 的配置写入 `~/.claude/settings.json` 的 `env` 字段。

## 2. 数据模型

**文件**: `src-tauri/src/models/token.rs`

```rust
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub api_key: String,                    // apiKey
    pub url: Option<String>,                // 自定义 API 地址
    pub default_sonnet_model: Option<String>, // Sonnet 模型映射
    pub default_opus_model: Option<String>,   // Opus 模型映射
    pub default_haiku_model: Option<String>,  // Haiku 模型映射
    pub custom_params: Option<HashMap<String, Value>>, // 自定义参数
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
}
```

## 3. 数据存储

| 文件 | 路径 | 说明 |
|------|------|------|
| Token 数据 | `~/.jadekit/tokens.json` | 通过 `app_paths::data_file("tokens.json")` |
| Claude 设置 | `~/.claude/settings.json` | Token 切换写入 `env` 字段 |

## 4. 切换机制

`switch_token` 将选中 Token 写入 Claude 运行时配置：

```
~/.claude/settings.json → env:
  ANTHROPIC_AUTH_TOKEN       = api_key
  ANTHROPIC_BASE_URL         = url（如有）
  ANTHROPIC_DEFAULT_SONNET_MODEL = default_sonnet_model（如有）
  ANTHROPIC_DEFAULT_OPUS_MODEL  = default_opus_model（如有）
  ANTHROPIC_DEFAULT_HAIKU_MODEL = default_haiku_model（如有）
```

## 5. Tauri 命令

| 命令 | 说明 |
|------|------|
| `get_tokens` | 获取所有 Token |
| `add_api_token` | 添加 Token |
| `update_api_token` | 更新 Token |
| `delete_api_token` | 删除 Token |
| `switch_api_token` | 切换活跃 Token |
| `move_api_token` | 拖拽排序 |
| `fetch_available_models` | 从 API 获取可用模型列表 |

## 6. 前端

- **页面**: `src/pages/ClaudePage.tsx`
- **Store**: `src/stores/useTokenStore.ts`
- **视图**: 表格/卡片双视图，搜索、拖拽排序

## 7. 兼容说明

`token_service.rs` 为兼容层，保留原有 API。新功能已迁移至 `provider_service.rs`（统一 Provider 管理），此模块将在适配完成后逐步废弃。
