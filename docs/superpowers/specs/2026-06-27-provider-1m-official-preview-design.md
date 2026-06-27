# 供应商增强设计：1M 上下文 / 官方订阅 / 配置预览可编辑

> 日期：2026-06-27
> 作用域：Provider 配置（`ProviderForm.tsx` + `provider_service.rs`）
> 对齐基准：cc-switch（`/Users/jiaxing/code/github/cc-switch`，同为 Tauri + React + Rust）

## 背景与问题

1. **1M 上下文未支持**：部分供应商（如 glm-4.6）已开放 1M 上下文档位，但 JadeKit 当前无便捷开关，用户需手填 `ANTHROPIC_BETAS` beta 串，且无法按模型角色精确控制。
2. **官方订阅 vs 自定义供应商冲突**：Claude/Codex 存在「订阅账号（Pro/Max，走 CLI 自带 OAuth 登录态）」与「API Key 自定义供应商」两种模式。一旦切换到自定义供应商，JadeKit 会把 `ANTHROPIC_AUTH_TOKEN`/`ANTHROPIC_BASE_URL` 写入 `~/.claude/settings.json`，按「本地文件优先」CLI 就走了 apikey 而非订阅。需要一个「官方订阅」选项，选中后清掉供应商配置，让 CLI 回落订阅登录。
3. **配置预览只读**：`ProviderForm` 右侧预览是从表单字段单向生成的只读 diff，用户无法直接编辑。

## 已验证的关键事实

- **`[1M]` 后缀是 Claude Code 官方机制**（非 cc-switch 私有）：写入 `ANTHROPIC_DEFAULT_SONNET_MODEL` 等的模型值后缀（如 `glm-4.6[1M]`），Claude Code 匹配前剥离后缀并开启 1M。适用于第三方供应商（走 `ANTHROPIC_BASE_URL`）。逃生阀 `CLAUDE_CODE_DISABLE_1M_CONTEXT=1`。
  来源：https://code.claude.com/docs/en/model-config
- **cc-switch 的 1M 判定**：per-model 的 `supports1m` 布尔，preset 数据驱动默认值 + 用户每行可勾，不按角色硬禁用。
- **cc-switch 的官方订阅**：固定 id 的特殊 Provider（`is_official_provider` 靠 id 判断），激活时移除供应商写入的认证字段让 CLI 回落 OAuth，不删 OAuth 凭证、不删 Provider 列表，可逆。
- **cc-switch 的配置编辑**：JsonEditor 作为单一事实源，结构化开关与 JSON 双向同步。

## 设计方案

实现分三阶段：① 1M 上下文 → ② 官方订阅 → ③ 预览可编辑。

### 模块 1：1M 上下文（per-model 声明）

**数据层（`src/types/provider.ts`）**
保留现有 4 个扁平模型字段，新增并行的能力声明对象（避免大改）：

```ts
// 每个模型角色是否声明 1M（对齐 cc-switch supports1m）
oneMContext?: {
  sonnet?: boolean;
  opus?: boolean;
  haiku?: boolean;
  reasoning?: boolean;
};
```

后端 `Provider` 结构（`src-tauri/src/models` / `provider.rs`）同步新增对应字段（serde rename 对齐前端驼峰）。

**写入层（`provider_service.rs` `merge_provider_to_env`）**
写 env 时，若某角色 `oneMContext` 为 true 且该角色模型值非空，给模型值拼 `[1M]` 后缀：
- `oneMContext.sonnet=true` + `default_sonnet_model="glm-4.6"` → `ANTHROPIC_DEFAULT_SONNET_MODEL="glm-4.6[1M]"`
- 模型值为空时不拼（避免写出孤立 `[1M]`）。
- 后缀常量集中定义（如 `const ONE_M_CONTEXT_SUFFIX: &str = "[1M]";`），不写魔法字符串。

**UI 层（`ProviderForm.tsx`）**
4 个 ModelComboBox 各加一个 `[1M]` checkbox（仅 `appType==='claude'` 显示）。默认值由 preset 决定，用户可改；不禁用任何角色（与 cc-switch 一致），带 hint 说明非所有上游支持。

**preset 层**
现有 `PRESETS` 扩展，preset 可携带 `oneMContext` 默认值；应用 preset 时回填 checkbox。

### 模块 2：官方订阅选项（Claude + Codex）

**形态**：内置两个固定 id 的特殊 Provider —— `__claude_official__`、`__codex_official__`，出现在 Provider 列表，带官方图标、不可删除/不可编辑。`isOfficialProvider(id)` 靠固定 id 判断（常量集中定义，对齐 cc-switch `is_official_provider`）。

**激活时的清除逻辑**（`provider_service.rs` 切换路径）
- Claude：读 `~/.claude/settings.json` → 从 `env` 移除供应商字段（清单集中为常量数组，与写入复用同一份）：
  `ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_BASE_URL`、`ANTHROPIC_DEFAULT_SONNET_MODEL`、`ANTHROPIC_DEFAULT_OPUS_MODEL`、`ANTHROPIC_DEFAULT_HAIKU_MODEL`、`ANTHROPIC_REASONING_MODEL` → 其它 env/配置原样保留 → 写回。
- Codex：同理清掉它写入的 apikey/base_url 配置项。
- **不动**：Provider 列表、OAuth 凭证（`.credentials.json`）。完全可逆。

**UI**：列表官方项点击即激活；toast 提示「已切回官方订阅登录，自定义供应商配置已从配置文件移除」。

### 模块 3：配置预览可编辑（第三阶段，独立交付）

把右侧只读 diff 预览升级为可编辑 JSON（单一事实源思路），结构化控件（1M checkbox、快捷开关）与 JSON 双向同步：
- 勾选/改字段 → parse → 改对象 → stringify 回写编辑器。
- 手改 JSON → parse 失败给错误提示，成功则反映回控件。

本阶段不阻塞前两个模块。

## 错误处理

- 1M：模型值为空时忽略 1M 标记；后端写入失败沿用现有错误返回路径。
- 官方订阅：settings.json 读取/写回失败给明确错误；移除字段对不存在的 key 是幂等的。
- 预览编辑：JSON parse 失败时保留用户输入并标红提示，不静默吞错。

## 测试

- 后端单测：`merge_provider_to_env` 拼 `[1M]` 后缀（含空值不拼）；官方订阅清除字段（含保留其它 env、幂等）。
- 前端：1M checkbox 默认值随 preset、可手动切换；官方 Provider 激活流程。
- 手动 e2e：切换到官方订阅后 `~/.claude/settings.json` 不含供应商字段，CLI `/context` 验证 1M 生效。

## 非目标（YAGNI）

- 不维护「哪个模型支持 1M」的智能白名单（交给 preset 数据 + 用户声明）。
- 不删除 OAuth 凭证、不删除 Provider 列表。
- 模块 3 不追求与 cc-switch 完全一致的 JsonEditor 组件，按需选型。
