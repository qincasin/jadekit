# 供应商增强（1M / 官方订阅 / 预览可编辑）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 JadeKit Provider 配置增加 per-model 1M 上下文声明、官方订阅特殊 Provider、可编辑配置预览，对齐 cc-switch。

**Architecture:** 1M 通过给模型值拼 `[1M]` 后缀写入 Claude env（Claude Code 官方机制）；官方订阅为固定 id 的特殊 Provider，激活时从 settings.json 移除供应商 env 字段让 CLI 回落 OAuth；预览升级为 JSON 可编辑、与结构化控件双向同步。前端 React 19 + TS，后端 Rust(Tauri)。

**Tech Stack:** React 19, TypeScript, zustand, Tauri 2, Rust, serde_json, i18next。

## Global Constraints

- 与用户沟通默认中文；新增代码必须补中文注释（尤其安全边界、状态流转、配置写入）。
- 不写魔法字符串：1M 后缀、官方 Provider id、要清除的 env 字段清单都必须集中为常量。
- 不动 StatusPanel.tsx（在途无关改动）。
- 设计基准见 `docs/superpowers/specs/2026-06-27-provider-1m-official-preview-design.md`。
- 对齐基准 cc-switch 源码：`/Users/jiaxing/code/github/cc-switch`。
- 提交前：`cd src-tauri && cargo test`（后端）、`yarn lint`（前端）、`git diff --check`。
- 频繁提交：每个 task 一次 commit。

---

## 阶段一：1M 上下文（per-model 声明）

### Task 1: 后端常量 + 模型字段拼接 `[1M]` 后缀

**Files:**
- Modify: `src-tauri/src/models/provider.rs:46`（在 `default_reasoning_model` 后新增字段）
- Modify: `src-tauri/src/services/provider_service.rs`（新增常量 + 改 `sync_to_claude_settings` 与 `preview_claude` 的 optional_fields 构造；测试 mod 在 `:1249`）
- Test: `src-tauri/src/services/provider_service.rs`（同文件 `#[cfg(test)] mod tests`）

**Interfaces:**
- Produces: `pub const ONE_M_CONTEXT_SUFFIX: &str = "[1M]";`
- Produces: `Provider.one_m_context: Option<OneMContext>`，其中
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, Default)]
  pub struct OneMContext {
      #[serde(default)] pub sonnet: bool,
      #[serde(default)] pub opus: bool,
      #[serde(default)] pub haiku: bool,
      #[serde(default)] pub reasoning: bool,
  }
  ```
- Produces: `fn model_with_1m(model: &Option<String>, enabled: bool) -> Option<String>` —— enabled 且非空时返回 `Some("model[1M]")`，否则原样返回。

- [ ] **Step 1: 写失败测试**（加入 `mod tests`）

```rust
#[test]
fn test_model_with_1m_appends_suffix() {
    // 启用且模型非空 → 拼后缀
    assert_eq!(
        model_with_1m(&Some("glm-4.6".to_string()), true),
        Some("glm-4.6[1M]".to_string())
    );
    // 启用但模型为空 → 不拼（避免孤立 [1M]）
    assert_eq!(model_with_1m(&None, true), None);
    assert_eq!(model_with_1m(&Some("".to_string()), true), Some("".to_string()));
    // 未启用 → 原样
    assert_eq!(
        model_with_1m(&Some("glm-4.6".to_string()), false),
        Some("glm-4.6".to_string())
    );
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test test_model_with_1m_appends_suffix`
Expected: 编译失败（`model_with_1m`/`OneMContext` 未定义）

- [ ] **Step 3: 新增结构与常量**

在 `src-tauri/src/models/provider.rs`，`Provider` 结构 `default_reasoning_model` 字段后加入：
```rust
    #[serde(rename = "oneMContext", skip_serializing_if = "Option::is_none")]
    pub one_m_context: Option<OneMContext>,
```
并在文件中新增（紧跟 `ProviderProxyConfig` 之后）：
```rust
/// 每个模型角色是否声明 1M 上下文能力（对齐 cc-switch supports1m）。
/// 写入 Claude env 时通过给模型名拼 `[1M]` 后缀生效（Claude Code 官方机制）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OneMContext {
    #[serde(default)]
    pub sonnet: bool,
    #[serde(default)]
    pub opus: bool,
    #[serde(default)]
    pub haiku: bool,
    #[serde(default)]
    pub reasoning: bool,
}
```
修复 `ProvidersConfig`/`From<ApiToken>`（`:85` 附近）等构造处：补 `one_m_context: None,`。

- [ ] **Step 4: 实现常量与 helper + 接入写入**

在 `provider_service.rs` 顶部常量区加入：
```rust
/// 1M 上下文模型后缀（Claude Code 官方机制，匹配前会被剥离）。
pub const ONE_M_CONTEXT_SUFFIX: &str = "[1M]";
```
新增 helper（放在 `merge_provider_to_env` 附近）：
```rust
/// 启用 1M 且模型非空时给模型值拼 `[1M]` 后缀，否则原样返回。
/// 中文注释：空模型不拼，避免写出孤立的 `[1M]` 触发上游报错。
fn model_with_1m(model: &Option<String>, enabled: bool) -> Option<String> {
    match model {
        Some(m) if enabled && !m.trim().is_empty() => Some(format!("{m}{ONE_M_CONTEXT_SUFFIX}")),
        other => other.clone(),
    }
}
```
在 `sync_to_claude_settings`（`:1122`）和 `preview_claude`（`:904` 附近，optional_fields 同构）把模型项改为经 `model_with_1m` 处理。示例（两处一致）：
```rust
    let one_m = provider.one_m_context.clone().unwrap_or_default();
    let sonnet = model_with_1m(&provider.default_sonnet_model, one_m.sonnet);
    let opus = model_with_1m(&provider.default_opus_model, one_m.opus);
    let haiku = model_with_1m(&provider.default_haiku_model, one_m.haiku);
    let reasoning = model_with_1m(&provider.default_reasoning_model, one_m.reasoning);
    let optional_fields = [
        ("ANTHROPIC_BASE_URL", &provider.url),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", &sonnet),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", &opus),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", &haiku),
        ("ANTHROPIC_REASONING_MODEL", &reasoning),
    ];
```
同时 `test_provider`（`:1254`）构造体补 `one_m_context: None,`。

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test test_model_with_1m_appends_suffix && cargo test`
Expected: 全部 PASS

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/models/provider.rs src-tauri/src/services/provider_service.rs
git commit -m "feat(provider): 后端支持 per-model 1M 上下文 [1M] 后缀

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 2: 前端类型 + 1M checkbox UI + preset 默认值

**Files:**
- Modify: `src/types/provider.ts`（`Provider` 接口加 `oneMContext`）
- Modify: `src/components/providers/ProviderForm.tsx`（state、save、preview、4 个 ModelComboBox 旁 checkbox、PRESETS）
- Modify: `src/locales/*`（1M hint 文案，跟随现有 i18n 结构）

**Interfaces:**
- Consumes: 后端 `oneMContext` serde 字段。
- Produces: `Provider.oneMContext?: { sonnet?: boolean; opus?: boolean; haiku?: boolean; reasoning?: boolean }`

- [ ] **Step 1: 扩展类型**

`src/types/provider.ts` 的 `Provider` 接口 `defaultReasoningModel?` 后加：
```ts
    /** 每个模型角色是否声明 1M 上下文（写入时拼 [1M] 后缀） */
    oneMContext?: {
        sonnet?: boolean;
        opus?: boolean;
        haiku?: boolean;
        reasoning?: boolean;
    };
```

- [ ] **Step 2: 表单 state + 持久化**

`ProviderForm.tsx`：新增 state（仿现有模型 state，`:192` 附近）
```ts
const [oneMContext, setOneMContext] = useState(editingProvider?.oneMContext || {});
```
在 `useEffect` 重置块（`:224`）加 `setOneMContext(editingProvider?.oneMContext || {});`
在 `handleSave` 的 `data`（`:329`）加：
```ts
oneMContext: Object.values(oneMContext).some(Boolean) ? oneMContext : undefined,
```
在 `buildPreviewProvider`（`:398`）返回对象加 `oneMContext`。

- [ ] **Step 3: ModelComboBox 旁加 1M checkbox**

在 4 个 `<ModelComboBox>`（`:608-635`）每个外面包一层，仅 `appType==='claude'` 时在标签行右侧渲染 checkbox。以 Sonnet 为例：
```tsx
<div className="space-y-1">
    <ModelComboBox label="Sonnet Model" placeholder="claude-sonnet-4-..."
        value={defaultSonnetModel} onChange={(v) => setDefaultSonnetModel(v)} options={fetchedModels} />
    {appType === 'claude' && (
        <label className="flex items-center gap-1.5 text-[11px] text-gray-500 dark:text-slate-400 cursor-pointer"
            title={t('providers.oneMHint', '声明该模型支持 1M 上下文（写入时拼 [1M] 后缀，需上游支持）')}>
            <input type="checkbox" className="h-3.5 w-3.5 rounded border-gray-300 dark:border-slate-600"
                checked={!!oneMContext.sonnet}
                onChange={(e) => setOneMContext(prev => ({ ...prev, sonnet: e.target.checked }))} />
            <span>声明支持 1M</span>
        </label>
    )}
</div>
```
对 opus / haiku / reasoning 重复（字段分别 `opus` / `haiku` / `reasoning`）。

- [ ] **Step 4: PRESETS 支持 1M 默认值**

`PRESETS`（`:173`）每项可选加 `oneMContext`；`applyPreset`（`:276`）里若 `preset.oneMContext` 存在则 `setOneMContext(preset.oneMContext)`。本次给 `Claude Official` 设 `{ sonnet: true, opus: true }`（官方 Sonnet/Opus 支持 1M），OpenRouter 不设。

- [ ] **Step 5: lint + 手动验证**

Run: `yarn lint`
Expected: 无新增报错。
手动：打开添加 Provider，勾选 Sonnet 的 1M，右侧预览应出现 `ANTHROPIC_DEFAULT_SONNET_MODEL` 带 `[1M]`。

- [ ] **Step 6: 提交**

```bash
git add src/types/provider.ts src/components/providers/ProviderForm.tsx src/locales
git commit -m "feat(provider): 表单按模型角色声明 1M 上下文

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## 阶段二：官方订阅特殊 Provider

### Task 3: 后端官方 Provider 常量 + 切换清除逻辑

**Files:**
- Modify: `src-tauri/src/services/provider_service.rs`（常量、`sync_to_claude_settings` 与 codex 分支官方分流、清除 helper、测试）

**Interfaces:**
- Produces: `pub const CLAUDE_OFFICIAL_PROVIDER_ID: &str = "__claude_official__";`
- Produces: `pub const CODEX_OFFICIAL_PROVIDER_ID: &str = "__codex_official__";`
- Produces: `pub fn is_official_provider(id: &str) -> bool`
- Produces: `const CLAUDE_PROVIDER_ENV_KEYS: &[&str]`（要清除的 env 字段，与写入复用同一份语义）

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn test_official_provider_clears_claude_env() {
    use serde_json::json;
    let mut env = json!({
        "ANTHROPIC_AUTH_TOKEN": "sk-x",
        "ANTHROPIC_BASE_URL": "https://x",
        "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-4.6[1M]",
        "KEEP_ME": "yes"
    });
    clear_claude_provider_env(env.as_object_mut().unwrap());
    assert!(env.get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert!(env.get("ANTHROPIC_BASE_URL").is_none());
    assert!(env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none());
    // 无关字段保留
    assert_eq!(env.get("KEEP_ME").and_then(|v| v.as_str()), Some("yes"));
    // 幂等：再清一次不报错
    clear_claude_provider_env(env.as_object_mut().unwrap());
}

#[test]
fn test_is_official_provider() {
    assert!(is_official_provider(CLAUDE_OFFICIAL_PROVIDER_ID));
    assert!(is_official_provider(CODEX_OFFICIAL_PROVIDER_ID));
    assert!(!is_official_provider("user-123"));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test test_official_provider_clears_claude_env test_is_official_provider`
Expected: 编译失败（符号未定义）

- [ ] **Step 3: 实现常量 + helper**

`provider_service.rs` 常量区：
```rust
/// 官方订阅特殊 Provider 的固定 id（对齐 cc-switch is_official_provider 思路）。
pub const CLAUDE_OFFICIAL_PROVIDER_ID: &str = "__claude_official__";
pub const CODEX_OFFICIAL_PROVIDER_ID: &str = "__codex_official__";

/// 切到官方订阅时需从 ~/.claude/settings.json 的 env 移除的供应商字段。
/// 中文注释：移除后 CLI 找不到 apikey/base_url，回落自带 OAuth 订阅登录态。
const CLAUDE_PROVIDER_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_REASONING_MODEL",
];

pub fn is_official_provider(id: &str) -> bool {
    id == CLAUDE_OFFICIAL_PROVIDER_ID || id == CODEX_OFFICIAL_PROVIDER_ID
}

/// 从 env 移除所有供应商字段（幂等）。
fn clear_claude_provider_env(env: &mut serde_json::Map<String, serde_json::Value>) {
    for key in CLAUDE_PROVIDER_ENV_KEYS {
        env.remove(*key);
    }
}
```

- [ ] **Step 4: 切换分流**

在 `sync_to_claude_settings`（`:1095`）函数开头，读取 settings 后、合并 provider 配置前插入：
```rust
    // 官方订阅：清除供应商 env 字段，让 CLI 回落 OAuth 订阅登录态后直接写回
    if is_official_provider(&provider.id) {
        if settings.get("env").is_none() {
            settings["env"] = serde_json::json!({});
        }
        if let Some(env) = settings["env"].as_object_mut() {
            clear_claude_provider_env(env);
        }
        return crate::services::storage::json_store::write_json(&settings_path, &settings);
    }
```
在 `sync_to_codex_config` 对应入口加同构官方分流（清 codex 写入的 apikey/base_url 配置项；参照该函数现有写入字段决定清除项，集中为 `const CODEX_PROVIDER_KEYS`）。

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test`
Expected: 全部 PASS

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/services/provider_service.rs
git commit -m "feat(provider): 官方订阅切换清除供应商 env 字段回落 OAuth

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 4: 前端内置官方 Provider + 列表激活

**Files:**
- Modify: `src/stores/useProviderStore.ts`（注入两个内置官方 Provider，激活走 switch）
- Modify: Provider 列表组件（官方项不可删/不可编辑、官方图标）
- Modify: `src/locales/*`（官方订阅文案、激活 toast）

**Interfaces:**
- Consumes: 后端 `is_official_provider` 行为（按 id）。
- Produces: 前端常量 `CLAUDE_OFFICIAL_PROVIDER_ID='__claude_official__'`、`CODEX_OFFICIAL_PROVIDER_ID='__codex_official__'`（集中在 `src/config/`）。

- [ ] **Step 1: 前端常量**

新建/复用 `src/config/providerConstants.ts`：
```ts
export const CLAUDE_OFFICIAL_PROVIDER_ID = '__claude_official__';
export const CODEX_OFFICIAL_PROVIDER_ID = '__codex_official__';
export const OFFICIAL_PROVIDER_IDS = [CLAUDE_OFFICIAL_PROVIDER_ID, CODEX_OFFICIAL_PROVIDER_ID];
export const isOfficialProvider = (id: string) => OFFICIAL_PROVIDER_IDS.includes(id);
```

- [ ] **Step 2: store 注入内置官方 Provider**

在 `useProviderStore` 的 provider 列表读取后，把两个官方 Provider（`appType: 'claude'|'codex'`，`name: 'Claude 官方订阅'|'Codex 官方订阅'`，固定 id，`apiKey: ''`，`icon` 官方）合并到展示列表（置顶）。激活时调用现有 `switch_provider`，后端按 id 走官方清除分支。

- [ ] **Step 3: 列表 UI 守卫**

列表渲染处：`isOfficialProvider(p.id)` 时隐藏删除/编辑入口，显示官方徽标；点击激活后 `showToast('已切回官方订阅登录，自定义供应商配置已从配置文件移除')`。

- [ ] **Step 4: lint + 手动验证**

Run: `yarn lint`
手动：激活「Claude 官方订阅」，检查 `~/.claude/settings.json` 的 env 不含 `ANTHROPIC_AUTH_TOKEN`/`ANTHROPIC_BASE_URL`；再激活某自定义 Provider，字段恢复写入。

- [ ] **Step 5: 提交**

```bash
git add src/config/providerConstants.ts src/stores/useProviderStore.ts src/components/providers src/locales
git commit -m "feat(provider): 内置官方订阅 Provider 与列表激活

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## 阶段三：配置预览可编辑

### Task 5: 预览升级为可编辑 JSON + 双向同步

**Files:**
- Modify: `src/components/providers/ProviderForm.tsx`（右侧预览块 `:794-836`）
- 可选 Create: `src/components/providers/EditablePreview.tsx`（拆分预览编辑组件，保持 ProviderForm 聚焦）

**Interfaces:**
- Consumes: `previewData`（现有 `[{title, content, originalContent}]`）、`internalSettings`、`oneMContext`。
- Produces: 受控可编辑预览，编辑内容能回写到 `internalSettings`/相关字段；parse 失败给错误态。

- [ ] **Step 1: 抽出 EditablePreview 组件（只读基线先跑通）**

新建 `EditablePreview.tsx`，props `{ files, onEdit }`，先把现有只读 diff 渲染原样搬入，`ProviderForm` 引用它，保证行为不变。Run `yarn lint`。

- [ ] **Step 2: 加入编辑态**

每个预览文件块加「编辑」切换：只读时显示 diff，编辑时显示 `<textarea>`（受控，值为 `content`）。本地维护 `draft` 与 `parseError`。

- [ ] **Step 3: 双向同步（编辑 → 结构化）**

textarea onChange：`JSON.parse(draft)`，
- 失败：setParseError，标红提示，不回写。
- 成功：从解析结果提取已知字段（env 里的模型/betas/headers 等）回写到 `internalSettings`（仅白名单字段，复用 `CLAUDE_SETTINGS_DEFAULTS` key 集），清除 parseError。

- [ ] **Step 4: 结构化 → 编辑同步**

沿用现有 effect：`internalSettings`/字段变化触发 `preview_provider_sync`，刷新 `files`，未处于编辑中或无 parseError 时更新 textarea draft（编辑中保留用户输入，避免覆盖）。

- [ ] **Step 5: lint + 手动验证**

Run: `yarn lint`
手动：编辑预览改一个 env 值 → 对应结构化控件更新；输入非法 JSON → 标红且不污染表单；勾 1M checkbox → 预览文本同步出现 `[1M]`。

- [ ] **Step 6: 提交**

```bash
git add src/components/providers/ProviderForm.tsx src/components/providers/EditablePreview.tsx
git commit -m "feat(provider): 配置预览可编辑并与表单双向同步

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- 模块1 1M → Task 1（后端写入）+ Task 2（前端 UI/preset）✓
- 模块2 官方订阅 → Task 3（后端清除）+ Task 4（前端内置 Provider）✓
- 模块3 预览可编辑 → Task 5 ✓
- 「不碰 OAuth/列表」「清单常量复用」「固定 id 判断」均在 Task 3 体现 ✓

**Placeholder scan:** 各 step 含具体代码/命令，无 TBD/TODO。Codex 清除项标注「参照现有写入字段决定」属定位指引（codex 写入逻辑在 `sync_to_codex_config`，实现时读取该函数现状）——非占位，给出了明确来源。

**Type consistency:** `one_m_context`/`oneMContext`、`OneMContext{sonnet,opus,haiku,reasoning}`、`model_with_1m`、`is_official_provider`、`clear_claude_provider_env`、`CLAUDE_OFFICIAL_PROVIDER_ID` 前后一致；前端 `isOfficialProvider` 与后端同名同义。
