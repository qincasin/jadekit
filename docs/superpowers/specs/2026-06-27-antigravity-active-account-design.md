# Antigravity 激活态单一真相模型 设计

## 背景

jadekit 的 Antigravity 账号功能是从上游 Antigravity-Manager 抄过来的,但激活态的数据模型与上游产生了偏离,导致「两份真相不一致」的 bug。本方案对齐上游的「单一真相源 + 原子切换」模型,彻底消除不一致。

## 根因分析

### 上游 Antigravity-Manager 的模型(无 bug)

上游 `Account` / `AccountSummary` 结构体**没有 `is_active` 字段**。「当前账号」是文件里的单一字段:

- 存储:`~/.antigravity_tools/accounts.json` 的 `current_account_id` 字段(唯一真相源)。
- 读:`get_current_account_id()`(`account.rs:1369`)= 纯文件读,零网络。
- 写:`switch_account`(`account.rs:940`)= **进程操作在前**(`:1000 integration.on_account_switch`)→ **持锁原子写状态在后**(`:1004-1009` 写 `current_account_id`)。
- 前端:独立的 `currentAccount` + `get_current_account` 命令,不靠每行的 active 列。

上游免疫的原因:**只有一份真相 + 切换原子**。上游也不感知「用户在 ide 内部直接切账号」—— 这是设计选择,不是 bug。

### jadekit 的偏离(有 bug)

jadekit 维护了**两份真相**:SQLite 的 `is_active` 列 **和** `~/.antigravity_tools/accounts.json` 的 `current_account_id`。而且:

- `~/.antigravity_tools/accounts.json` 在 jadekit 里是**只写不读的死数据**(`switch_account` 写,但 `list_accounts` / 任何读路径都不读它)。
- `switch_account`(`antigravity_service.rs:635`)写回顺序非原子:先写 `accounts.json`(:654)→ 进程操作(:666)→ 最后写 DB `is_active`(:694)。进程操作失败时 `accounts.json` 被 restore 回滚(:683),但 **DB `is_active` 不回滚** → 出现一致性问题。

## 决策

- **不兼容**独立的 Antigravity Manager 桌面 app(不再写 `~/.antigravity_tools/accounts.json`)。
- jadekit **自己实现一套**,完全仿照上游「单一真相源 + 原子切换」思路,但用 jadekit 自己的文件。
- 真相文件放在 jadekit 自己的数据目录下:`~/.jadekit/antigravity/`(与 `~/.jadekit/jadekit.db`、`webdav.json` 等同体系)。
- 不引入 keychain 读取、不引入 userinfo 网络探测(放弃备忘里的旧方案)。语义与上游一致:**不感知** ide 内部切换。

## 数据层

### 真相文件:`~/.jadekit/antigravity/current-account.json`

仿照上游 `AccountIndex.current_account_id`,极简单字段真相:

```json
{
  "currentAccountId": "uuid-of-active-account",
  "updatedAt": 1719480000
}
```

- 只存当前账号的 id + 更新时间戳。
- 文件不存在 / 解析失败 → 视为「无当前账号」,静默兜底(回退 DB 现状,不阻塞列表)。
- 纯文件读写,零网络。

### Rust:真相文件读写(新增,放 `src-tauri/src/services/ag_current_account.rs` 或并入 `antigravity_service.rs`)

仿照上游 `get_current_account_id` / `set_current_account_id`(`account.rs:1369 / 1384`):

```rust
/// 读真相:返回当前账号 id(纯文件读)。
/// 文件缺失/损坏 → Ok(None),不报错。
fn get_current_account_id() -> Result<Option<String>, String>

/// 写真相:原子写文件(temp → rename)+ 同步刷 DB is_active。
/// 对齐上游 account.rs:1003-1009 的持锁原子语义:文件与 DB 在同一调用内一致更新。
fn set_current_account_id(db: &Arc<Database>, id: &str) -> Result<(), String>
```

`set_current_account_id` 内部:**先写真相文件(temp → rename),再用 DB 事务一次性刷 `is_active`**(命中的置 1、其余置 0,复用现有 `db.set_active_antigravity_account`)。

## 业务层修复

### Bug A:激活态对账 —— `list_accounts`(`antigravity_service.rs:391`)

惰性对账(读时静默修正 DB),所有读入口都走 `list_accounts`,改动收敛在后端一处:

```rust
pub fn list_accounts(db: &Arc<Database>) -> Result<Vec<AntigravityAccount>, String> {
    let mut accounts = db.list_antigravity_accounts()?;
    if let Ok(Some(current_id)) = get_current_account_id() {
        let needs_fix = accounts.iter().any(|a| {
            (a.id == current_id) != a.is_active
        });
        if needs_fix {
            let _ = db.set_active_antigravity_account(&current_id);
            for a in accounts.iter_mut() {
                a.is_active = a.id == current_id;
            }
        }
    }
    Ok(accounts)
}
```

文件缺失/损坏 → `get_current_account_id` 返回 `None` → 跳过对账,返回 DB 现状(兜底,不阻塞)。

### Bug B:切换原子化 —— `switch_account`(`antigravity_service.rs:635`)

把「先写外部 `accounts.json` → 进程操作 → 后写 DB」改成上游顺序:**进程操作在前,状态写回在后且原子**:

```
改后(对齐上游 account.rs:999-1009):
1. 刷新 token(已有,:646)
2. 进程操作:关闭→注入→重启(已有,:666)  ← 失败直接 return,不动状态
3. set_current_account_id(db, id)         ← 原子:写 jadekit 真相文件 + 刷 DB is_active,一步
```

- 进程操作失败 → 直接返回错误,真相文件与 DB **都不动**(切换没成功就不改激活态)。
- 删除对外部 `~/.antigravity_tools/accounts.json` 的双写:`set_antigravity_manager_current_account`(:711)、`restore_antigravity_manager_current_account`(:764)、`write_antigravity_manager_accounts`(:779)三个函数及其在 `switch_account` 中的调用(:654 / :683)。

### Bug C:删除当前账号清理 —— `delete_account`

删的如果是当前账号,真相文件回退到第一个账号(或清空),对齐上游 `account.rs:842-843 / 877-878`。

## UI 层

**无需改动。** 前端只消费 `isActive` 字段,后端对账后值正确,5 处展示面自动正确:

- `src/components/antigravity/AccountCard.tsx`(高亮/徽章)
- `src/components/antigravity/AccountDetailsDialog.tsx`(状态点)
- `src/pages/AntigravityPage.tsx`(行样式/「活跃」徽章)
- `src/pages/Dashboard.tsx:233`(仪表盘当前账号邮箱)
- 后端 `db.get_active_antigravity_account()`(`antigravity_accounts.rs:115`,读 `is_active`,对账后自然正确)

这是惰性对账方案的核心收益:改动收敛在后端,UI 零改动回归。

## 测试

### 单元测试(Rust,仿上游 `account.rs:188-310` 测试风格,用 tempdir 隔离)

- `get_current_account_id`:文件不存在 → `Ok(None)`;合法 → `Ok(Some(id))`;损坏 → `Ok(None)`。
- `set_current_account_id`:文件内容正确;DB `is_active` 命中=1、其余=0;temp+rename 原子。
- `list_accounts` 对账:构造「DB 说 A 活、文件说 B 活」→ 调用后 DB 修正为 B 活、返回值一致。
- `switch_account` 失败回滚:进程操作失败 → 真相文件与 DB `is_active` 都保持原状。
- `delete_account` 清理:删当前账号 → 真相文件回退到第一个账号。

### 手动验证清单

1. 正常切换:列表 / Dashboard / 详情弹窗都显示新账号活跃,`current-account.json` = 新 id。
2. 重启 app:活跃态与切换后一致(惰性对账从文件读回)。
3. 进程切换失败:报错后活跃态不变,真相文件与 DB 一致。
4. 删当前账号:活跃态回退到第一个,不出现「无活跃」或「两个都活」。
5. 数据展示面回归:`AccountCard` / `AccountDetailsDialog` / `AntigravityPage` / `Dashboard.tsx:233` 四处一致。
6. 文件损坏兜底:`current-account.json` 写乱码 → 列表不崩,回退 DB 现状。

## 不在范围

- ❌ keychain 读取、userinfo 网络探测(放弃备忘旧方案)。
- ❌ 感知「用户在 ide 内部直接切账号」(语义与上游一致,不感知)。
- ❌ chat 模型列表动态化、供应商 1M 上下文、图片位置编辑(非本次需求)。

## 数据流

```
切换: switch_account
  → 刷新 token → 进程操作(失败即返回,不动状态)
  → set_current_account_id(db, id)
       → 写 ~/.jadekit/antigravity/current-account.json (temp→rename)
       → DB 事务: is_active 命中=1, 其余=0

读取(列表/Dashboard/详情): list_accounts
  → DB 读出 accounts
  → get_current_account_id() 读真相文件
  → 不一致则静默修正 DB is_active + 返回值
  → 前端消费 isActive(5 处展示面一致)

删除: delete_account
  → 若删的是当前账号 → 真相文件回退第一个账号 + 刷 DB
```
