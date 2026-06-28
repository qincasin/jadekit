// Task 6 是增量落地：Store / TaskListFilter 及其方法在 Task 7-9（messages CRUD、
// Coordinator、Tauri 命令）接入前，于 lib 非 test 构建中尚无消费者，故此处允许
// dead_code 以避免噪音。test 构建中所有公共方法均被覆盖。
#![allow(dead_code)]

//! Hermes Store —— SQLite 编排状态机（移植 orca `db.ts`，Rust 化）。
//!
//! 设计来源：
//! - `docs/superpowers/specs/2026-06-27-helm-hermes-design.md` §6.4 / §7
//! - orca `runtime/orchestration/db.ts`（近 1:1 移植源）
//! - jadekit `database/mod.rs`（`Mutex<Connection>` + `Result<_, String>` 结构风格）
//!
//! 本模块只负责 **schema + 迁移 + Task/Dispatch CRUD + DAG ready 提升**。
//! messages / decision_gates / coordinator_runs 的 CRUD 与 `reconcile_on_startup`
//! 留给 Task 7（YAGNI）。schema 一次性建出全部表以保证 schema 是单块的。
//!
//! 并发不变量（最重要的正确性属性，见 orca `db.ts` line 543-547 注释）：
//! `update_task_status` 内部在同一写事务里调用 `promote_ready_tasks`，
//! 因此任一 task 完成 → 其下游 pending 任务在 **同一次提交** 内被提升为 ready，
//! 即使进程在「写完 status」和「promote」之间崩溃也不会留下孤儿 pending。
//! rusqlite 的 `Transaction`（可 deref 到 `Connection`）保证两者原子提交或一起回滚；
//! `Mutex<Connection>` 串行化所有写者，"同一事务 = 同一 `tx`"。

use crate::hermes::types::{
    AgentAssignment, DispatchContext, DispatchStatus, Task, TaskStatus,
};
use rusqlite::{params, Connection, Transaction};
use std::path::Path;
use std::sync::Mutex;

// =============================================================================
// 常量：表 / 列 / 索引 / PRAGMA（不写魔法串；所有标识符集中定义）
// =============================================================================

/// 当前 schema 版本号（对应 orca `SCHEMA_VERSION`）。
/// jadekit 从 v1 起步——所有列在初始 CREATE TABLE 中齐备。
const SCHEMA_VERSION: u32 = 1;

/// 表/列/索引名集中定义，避免 SQL 里散落魔法串（Task 7 的 messages/gates/runs CRUD
/// 会复用这些常量）。表名常量当前未被 SQL 字面量直接引用——schema 内联字面量与
/// orca/`database/schema.rs` 风格一致——但作为权威词汇表保留。
const TABLE_TASKS: &str = "tasks";
const TABLE_DISPATCH_CONTEXTS: &str = "dispatch_contexts";
const TABLE_MESSAGES: &str = "messages";
const TABLE_DECISION_GATES: &str = "decision_gates";
const TABLE_COORDINATOR_RUNS: &str = "coordinator_runs";

// tasks 列名（Task 7 复用）
const C_ID: &str = "id";
const C_PARENT_ID: &str = "parent_id";
const C_CREATED_BY_TERMINAL_HANDLE: &str = "created_by_terminal_handle";
const C_TASK_TITLE: &str = "task_title";
const C_DISPLAY_NAME: &str = "display_name";
const C_SPEC: &str = "spec";
const C_STATUS: &str = "status";
const C_DEPS: &str = "deps";
const C_RESULT: &str = "result";
const C_CREATED_AT: &str = "created_at";
const C_COMPLETED_AT: &str = "completed_at";
const C_ASSIGNMENT: &str = "assignment";

// dispatch_contexts 列名
const C_TASK_ID: &str = "task_id";
const C_ASSIGNEE_HANDLE: &str = "assignee_handle";
const C_FAILURE_COUNT: &str = "failure_count";
const C_LAST_FAILURE: &str = "last_failure";
const C_DISPATCHED_AT: &str = "dispatched_at";
const C_LAST_HEARTBEAT_AT: &str = "last_heartbeat_at";

/// 熔断阈值：累计失败次数达到此值 → `CircuitBroken`（对齐 orca）。
const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

// =============================================================================
// Store 结构
// =============================================================================

/// Hermes 编排状态机的 SQLite 句柄。所有写操作通过 `Mutex` 串行化；
/// 关键不变量（status 写 + ready 提升）在同一 `Transaction` 内提交。
pub struct Store {
    /// 内部 `Mutex<Connection>`，与 jadekit `database::Database` 同一风格。
    /// `Transaction` 由 `Connection::transaction` 创建后会借用 `&mut Connection`，
    /// 故仍需独占锁。
    conn: Mutex<Connection>,
}

/// `list_tasks` 的过滤条件，对齐 orca `listTasks(filter)`。
///
/// - `status`：按 status 列过滤（与 `ready` 互斥；`ready=true` 时优先使用 ready）。
/// - `ready`：仅返回 `Ready` 任务（用于 Coordinator 派发循环）。
#[derive(Debug, Clone, Default)]
pub struct TaskListFilter {
    pub status: Option<TaskStatus>,
    pub ready: bool,
}

impl Store {
    /// 打开文件型 Hermes 数据库。
    ///
    /// 健壮性 PRAGMA（对齐 §6.4 / orca `db.ts` 构造器）：
    /// - `journal_mode=WAL`：写不阻塞读，崩溃后 WAL 可前滚恢复——崩溃恢复意图。
    /// - `synchronous=NORMAL`：WAL 模式下安全；fsync 频率从 FULL 降低，性能换数据安全平衡点。
    /// - `busy_timeout=5000`：多写者争用时给 5s 缓冲，避免立即抛 SQLITE_BUSY。
    /// - `foreign_keys=ON`：未来 cross-table 约束开启（即使当前无 FK，也保持开启习惯）。
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn =
            Connection::open(path).map_err(|e| format!("Failed to open hermes store: {e}"))?;
        Self::init_connection(&conn)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.create_tables()?;
        Ok(store)
    }

    /// 测试用：内存数据库。WAL 在 `:memory:` 上是 no-op（rusqlite/better-sqlite3 一致），
    /// 用于逻辑层单测。`foreign_keys` 仍开启以保持行为一致。
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory hermes store: {e}"))?;
        Self::init_connection(&conn)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.create_tables()?;
        Ok(store)
    }

    fn init_connection(conn: &Connection) -> Result<(), String> {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("PRAGMA journal_mode=WAL failed: {e}"))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| format!("PRAGMA synchronous=NORMAL failed: {e}"))?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))
            .map_err(|e| format!("PRAGMA busy_timeout=5000 failed: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("PRAGMA foreign_keys=ON failed: {e}"))?;
        Ok(())
    }

    /// 一次性创建全部 5 张表 + 索引。schema 是单块的——即使本任务不实现
    /// messages/gates/runs 的 CRUD，也先把表建出来，避免后续 task 反复回头改 schema。
    fn create_tables(&self) -> Result<(), String> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to BEGIN create_tables tx: {e}"))?;
        Self::create_tables_inner(&tx)?;
        tx.commit()
            .map_err(|e| format!("Failed to COMMIT create_tables tx: {e}"))?;
        Ok(())
    }

    fn create_tables_inner(tx: &Transaction) -> Result<(), String> {
        // ── messages ──
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                id            TEXT NOT NULL,
                from_handle   TEXT NOT NULL,
                to_handle     TEXT NOT NULL,
                subject       TEXT NOT NULL,
                body          TEXT NOT NULL DEFAULT '',
                type          TEXT NOT NULL DEFAULT 'status'
                  CHECK(type IN (
                    'status', 'dispatch', 'worker_done', 'merge_ready',
                    'escalation', 'handoff', 'decision_gate', 'heartbeat'
                  )),
                priority      TEXT NOT NULL DEFAULT 'normal'
                  CHECK(priority IN ('normal', 'high', 'urgent')),
                thread_id     TEXT,
                payload       TEXT,
                read          INTEGER NOT NULL DEFAULT 0,
                sequence      INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at    TEXT NOT NULL DEFAULT (datetime('now')),
                delivered_at  TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_id ON messages(id);
            CREATE INDEX IF NOT EXISTS idx_inbox ON messages(to_handle, read);
            CREATE INDEX IF NOT EXISTS idx_thread ON messages(thread_id);
            CREATE INDEX IF NOT EXISTS idx_messages_undelivered_inbox
              ON messages(to_handle, read, delivered_at, sequence);
            ",
        )
        .map_err(|e| format!("Failed to create messages table: {e}"))?;

        // ── tasks ──
        // Hermes 增量：assignment TEXT 列（AgentAssignment 的 JSON 序列化）。NULL 表示未选兵。
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                id                          TEXT PRIMARY KEY,
                parent_id                   TEXT,
                created_by_terminal_handle  TEXT,
                task_title                  TEXT,
                display_name                TEXT,
                spec                        TEXT NOT NULL,
                status                      TEXT NOT NULL DEFAULT 'pending'
                  CHECK(status IN (
                    'pending', 'ready', 'dispatched',
                    'completed', 'failed', 'blocked'
                  )),
                deps                        TEXT NOT NULL DEFAULT '[]',
                result                      TEXT,
                assignment                  TEXT,
                created_at                  TEXT NOT NULL DEFAULT (datetime('now')),
                completed_at                TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_tasks_parent ON tasks(parent_id);
            ",
        )
        .map_err(|e| format!("Failed to create tasks table: {e}"))?;

        // ── dispatch_contexts ──
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS dispatch_contexts (
                id                  TEXT PRIMARY KEY,
                task_id             TEXT NOT NULL,
                assignee_handle     TEXT,
                status              TEXT NOT NULL DEFAULT 'pending'
                  CHECK(status IN ('pending', 'dispatched', 'completed', 'failed', 'circuit_broken')),
                failure_count       INTEGER NOT NULL DEFAULT 0,
                last_failure        TEXT,
                dispatched_at       TEXT,
                completed_at        TEXT,
                created_at          TEXT NOT NULL DEFAULT (datetime('now')),
                last_heartbeat_at   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_dispatch_task ON dispatch_contexts(task_id);
            CREATE INDEX IF NOT EXISTS idx_dispatch_status ON dispatch_contexts(status);
            ",
        )
        .map_err(|e| format!("Failed to create dispatch_contexts table: {e}"))?;

        // ── decision_gates ──
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS decision_gates (
                id            TEXT PRIMARY KEY,
                task_id       TEXT NOT NULL,
                question      TEXT NOT NULL,
                options       TEXT NOT NULL DEFAULT '[]',
                status        TEXT NOT NULL DEFAULT 'pending'
                  CHECK(status IN ('pending', 'resolved', 'timeout')),
                resolution    TEXT,
                created_at    TEXT NOT NULL DEFAULT (datetime('now')),
                resolved_at   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_gates_task ON decision_gates(task_id);
            CREATE INDEX IF NOT EXISTS idx_gates_status ON decision_gates(status);
            ",
        )
        .map_err(|e| format!("Failed to create decision_gates table: {e}"))?;

        // ── coordinator_runs ──
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS coordinator_runs (
                id                  TEXT PRIMARY KEY,
                spec                TEXT NOT NULL,
                status              TEXT NOT NULL DEFAULT 'idle'
                  CHECK(status IN ('idle', 'running', 'completed', 'failed')),
                coordinator_handle  TEXT NOT NULL,
                poll_interval_ms    INTEGER NOT NULL DEFAULT 2000,
                created_at          TEXT NOT NULL DEFAULT (datetime('now')),
                completed_at        TEXT
            );
            ",
        )
        .map_err(|e| format!("Failed to create coordinator_runs table: {e}"))?;

        // schema 版本（jadekit 自有迁移起点；幂等设置 user_version）
        let current: u32 = tx
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);
        if current < SCHEMA_VERSION {
            tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
                .map_err(|e| format!("Failed to set user_version: {e}"))?;
        }
        Ok(())
    }

    // ── Task CRUD ──

    /// 写入一条任务。镜像 orca line 441：
    /// - deps 为空 → status = `Ready`
    /// - deps 非空 → status = `Pending`（待 `promote_ready_tasks` 提升）
    ///
    /// 注意：调用方提供的 `task.status` 字段会被覆盖；status 由 deps 推导，
    /// 这样调用方无需关心初始状态规则。
    /// deps 以 JSON 数组字符串持久化（对齐 orca `deps TEXT`）。
    pub fn create_task(&self, mut task: Task) -> Result<(), String> {
        let derived_status = if task.deps.is_empty() {
            TaskStatus::Ready
        } else {
            TaskStatus::Pending
        };
        task.status = derived_status;

        let deps_json = serde_json::to_string(&task.deps)
            .map_err(|e| format!("Failed to serialize deps: {e}"))?;
        let assignment_json = match &task.assignment {
            Some(a) => Some(
                serde_json::to_string(a).map_err(|e| format!("Failed to serialize assignment: {e}"))?,
            ),
            None => None,
        };

        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO tasks (
                id, parent_id, created_by_terminal_handle, task_title, display_name,
                spec, status, deps, result, assignment, created_at, completed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                task.id,
                task.parent_id,
                // Hermes 当前无 created_by_terminal_handle 概念，统一 NULL
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                task.spec,
                derived_status.as_str(),
                deps_json,
                task.result,
                assignment_json,
                task.created_at,
                task.completed_at,
            ],
        )
        .map_err(|e| format!("Failed to insert task: {e}"))?;
        Ok(())
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, parent_id, spec, status, deps, result, assignment,
                        created_at, completed_at
                 FROM tasks WHERE id = ?",
            )
            .map_err(|e| format!("Failed to prepare get_task: {e}"))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| format!("Failed to query get_task: {e}"))?;
        match rows.next().map_err(|e| format!("Failed to fetch row: {e}"))? {
            Some(row) => Ok(Some(Self::row_to_task(row)?)),
            None => Ok(None),
        }
    }

    pub fn list_tasks(&self, filter: TaskListFilter) -> Result<Vec<Task>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = if filter.ready {
            conn.prepare(
                "SELECT id, parent_id, spec, status, deps, result, assignment,
                        created_at, completed_at
                 FROM tasks WHERE status = 'ready' ORDER BY created_at",
            )
            .map_err(|e| format!("Failed to prepare list_tasks (ready): {e}"))?
        } else if let Some(s) = filter.status {
            let sql = format!(
                "SELECT id, parent_id, spec, status, deps, result, assignment,
                        created_at, completed_at
                 FROM tasks WHERE status = '{}' ORDER BY created_at",
                s.as_str()
            );
            conn.prepare(&sql).map_err(|e| format!("Failed to prepare list_tasks (status): {e}"))?
        } else {
            conn.prepare(
                "SELECT id, parent_id, spec, status, deps, result, assignment,
                        created_at, completed_at
                 FROM tasks ORDER BY created_at",
            )
            .map_err(|e| format!("Failed to prepare list_tasks (all): {e}"))?
        };
        let mut rows = stmt
            .query([])
            .map_err(|e| format!("Failed to query list_tasks: {e}"))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("Failed to fetch list_tasks row: {e}"))?
        {
            out.push(Self::row_to_task(row)?);
        }
        Ok(out)
    }

    fn row_to_task(row: &rusqlite::Row) -> Result<Task, String> {
        let id: String = row
            .get(0)
            .map_err(|e| format!("get_task.id: {e}"))?;
        let parent_id: Option<String> = row.get(1).ok();
        let spec: String = row
            .get(2)
            .map_err(|e| format!("get_task.spec: {e}"))?;
        let status_str: String = row
            .get(3)
            .map_err(|e| format!("get_task.status: {e}"))?;
        let deps_str: String = row
            .get(4)
            .map_err(|e| format!("get_task.deps: {e}"))?;
        let result: Option<String> = row.get(5).ok();
        let assignment_str: Option<String> = row.get(6).ok();
        let created_at: String = row
            .get(7)
            .map_err(|e| format!("get_task.created_at: {e}"))?;
        let completed_at: Option<String> = row.get(8).ok();

        let status = TaskStatus::from_str(&status_str)?;
        let deps: Vec<String> = serde_json::from_str(&deps_str)
            .map_err(|e| format!("Failed to deserialize deps '{deps_str}': {e}"))?;
        let assignment: Option<AgentAssignment> = match assignment_str {
            Some(s) if !s.is_empty() => Some(
                serde_json::from_str(&s)
                    .map_err(|e| format!("Failed to deserialize assignment '{s}': {e}"))?,
            ),
            _ => None,
        };

        Ok(Task {
            id,
            parent_id,
            spec,
            status,
            deps,
            result,
            assignment,
            created_at,
            completed_at,
        })
    }

    /// 更新任务状态。如果新状态是 `Completed`，则在 **同一写事务** 内
    /// 调用 [`promote_ready_tasks_in_tx`]，把所有 deps 已全部 Completed 的
    /// 下游 Pending 任务原子提升为 Ready（并发不变量；见模块级文档）。
    pub fn update_task_status(
        &self,
        id: &str,
        new_status: TaskStatus,
    ) -> Result<(), String> {
        let mut conn = lock_conn!(self.conn);
        // 关键：status 写 + ready 提升放同一 tx，要么一起提交要么一起回滚。
        let tx = conn
            .transaction()
            .map_err(|e| format!("BEGIN update_task_status tx failed: {e}"))?;
        Self::update_task_status_in_tx(&tx, id, new_status)?;
        tx.commit()
            .map_err(|e| format!("COMMIT update_task_status tx failed: {e}"))?;
        Ok(())
    }

    fn update_task_status_in_tx(
        tx: &Transaction,
        id: &str,
        new_status: TaskStatus,
    ) -> Result<(), String> {
        // completed_at 仅在终态写入；其它状态置 NULL 不影响。
        let now = chrono::Utc::now().to_rfc3339();
        let completed_at: Option<&str> =
            if matches!(new_status, TaskStatus::Completed | TaskStatus::Failed) {
                Some(&now)
            } else {
                None
            };
        let updated = tx
            .execute(
                "UPDATE tasks SET status = ?1, completed_at = COALESCE(?2, completed_at) WHERE id = ?3",
                params![new_status.as_str(), completed_at, id],
            )
            .map_err(|e| format!("UPDATE tasks failed: {e}"))?;
        if updated == 0 {
            return Err(format!("Task not found: {id}"));
        }
        // 完成后立即提升下游 pending 任务（同事务）。
        if matches!(new_status, TaskStatus::Completed) {
            Self::promote_ready_tasks_in_tx(tx, id)?;
        }
        Ok(())
    }

    /// 扫描所有 `pending` 任务；对于把 `completed_task_id` 列为依赖、
    /// 且全部 deps 都已 `completed` 的任务，原子提升为 `ready`。
    /// 移植 orca `promoteReadyTasks`（line 548-567），但以同事务 SQL 形式执行。
    fn promote_ready_tasks_in_tx(
        tx: &Transaction,
        completed_task_id: &str,
    ) -> Result<(), String> {
        // 取所有 pending 任务的 (id, deps_json)
        let mut stmt = tx
            .prepare("SELECT id, deps FROM tasks WHERE status = 'pending'")
            .map_err(|e| format!("prepare promote: {e}"))?;
        let candidates: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("query promote: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect promote: {e}"))?;
        drop(stmt);

        for (task_id, deps_json) in candidates {
            let deps: Vec<String> = serde_json::from_str(&deps_json)
                .map_err(|e| format!("promote: bad deps '{deps_json}': {e}"))?;
            if !deps.iter().any(|d| d == completed_task_id) {
                continue;
            }
            if deps.is_empty() {
                continue;
            }
            // 该任务所有依赖必须全部 completed 才提升。
            let all_done = Self::all_deps_completed(tx, &deps)?;
            if all_done {
                tx.execute(
                    "UPDATE tasks SET status = 'ready' WHERE id = ?1",
                    params![task_id],
                )
                .map_err(|e| format!("promote UPDATE failed for {task_id}: {e}"))?;
            }
        }
        Ok(())
    }

    fn all_deps_completed(tx: &Transaction, deps: &[String]) -> Result<bool, String> {
        if deps.is_empty() {
            return Ok(true);
        }
        // 用 placeholders IN 查询；任一 dep 不是 completed 就返回 false。
        let placeholders = std::iter::repeat("?")
            .take(deps.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT COUNT(*) FROM tasks
             WHERE id IN ({placeholders}) AND status = 'completed'"
        );
        let done_count: i64 = tx
            .query_row(
                &sql,
                rusqlite::params_from_iter(deps.iter().map(|s| s.as_str())),
                |r| r.get(0),
            )
            .map_err(|e| format!("all_deps_completed query: {e}"))?;
        Ok(done_count as usize == deps.len())
    }

    // ── Dispatch CRUD ──

    /// 写入一条派发上下文。
    pub fn create_dispatch(&self, ctx: DispatchContext) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO dispatch_contexts (
                id, task_id, assignee_handle, status, failure_count,
                last_failure, dispatched_at, completed_at, created_at, last_heartbeat_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                ctx.id,
                ctx.task_id,
                ctx.assignee,
                ctx.status.as_str(),
                ctx.failure_count,
                ctx.last_failure,
                ctx.dispatched_at,
                ctx.completed_at,
                ctx.created_at,
                ctx.last_heartbeat_at,
            ],
        )
        .map_err(|e| format!("Failed to insert dispatch_context: {e}"))?;
        Ok(())
    }

    pub fn get_dispatch(&self, id: &str) -> Result<Option<DispatchContext>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, assignee_handle, status, failure_count,
                        last_heartbeat_at, last_failure, dispatched_at, completed_at, created_at
                 FROM dispatch_contexts WHERE id = ?",
            )
            .map_err(|e| format!("prepare get_dispatch: {e}"))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| format!("query get_dispatch: {e}"))?;
        match rows
            .next()
            .map_err(|e| format!("fetch get_dispatch: {e}"))?
        {
            Some(row) => Ok(Some(Self::row_to_dispatch(row)?)),
            None => Ok(None),
        }
    }

    fn row_to_dispatch(row: &rusqlite::Row) -> Result<DispatchContext, String> {
        let id: String = row.get(0).map_err(|e| format!("dispatch.id: {e}"))?;
        let task_id: String = row.get(1).map_err(|e| format!("dispatch.task_id: {e}"))?;
        let assignee: Option<String> = row.get(2).ok();
        let status_str: String = row
            .get(3)
            .map_err(|e| format!("dispatch.status: {e}"))?;
        let failure_count: u32 = row
            .get::<_, i64>(4)
            .map_err(|e| format!("dispatch.failure_count: {e}"))?
            as u32;
        let last_heartbeat_at: Option<String> = row.get(5).ok();
        let last_failure: Option<String> = row.get(6).ok();
        let dispatched_at: Option<String> = row.get(7).ok();
        let completed_at: Option<String> = row.get(8).ok();
        let created_at: String = row
            .get(9)
            .map_err(|e| format!("dispatch.created_at: {e}"))?;

        Ok(DispatchContext {
            id,
            task_id,
            assignee,
            status: DispatchStatus::from_str(&status_str)?,
            failure_count,
            last_heartbeat_at,
            last_failure,
            dispatched_at,
            completed_at,
            created_at,
        })
    }

    /// 失败一次派发——`failure_count += 1`；达到 [`CIRCUIT_BREAKER_THRESHOLD`] (3) 次
    /// → status 置为 `CircuitBroken`（熔断），否则置为 `Failed`。
    /// 镜像 orca `failDispatch`（line 704-731），但任务侧的级联状态变化由调用方负责
    /// （YAGNI：本任务只做 dispatch 表的原子更新；task 侧的 Failed/Ready 由
    /// 后续 Coordinator / failActiveDispatchForTask 串联）。
    ///
    /// 返回更新后的 DispatchContext（便于调用方决定是否上报 Planner）。
    pub fn fail_dispatch(
        &self,
        id: &str,
        last_failure: &str,
    ) -> Result<Option<DispatchContext>, String> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| format!("BEGIN fail_dispatch tx: {e}"))?;
        let result = Self::fail_dispatch_in_tx(&tx, id, last_failure)?;
        tx.commit()
            .map_err(|e| format!("COMMIT fail_dispatch tx: {e}"))?;
        Ok(result)
    }

    fn fail_dispatch_in_tx(
        tx: &Transaction,
        id: &str,
        last_failure: &str,
    ) -> Result<Option<DispatchContext>, String> {
        // 先读取当前 failure_count（同事务内可见）。
        let current_failure_count: Option<i64> = tx
            .query_row(
                "SELECT failure_count FROM dispatch_contexts WHERE id = ?",
                params![id],
                |r| r.get(0),
            )
            .ok();
        let Some(current_failure_count) = current_failure_count else {
            return Ok(None);
        };
        let new_count: u32 = current_failure_count as u32 + 1;
        let new_status = if new_count >= CIRCUIT_BREAKER_THRESHOLD {
            DispatchStatus::CircuitBroken
        } else {
            DispatchStatus::Failed
        };
        tx.execute(
            "UPDATE dispatch_contexts
             SET status = ?1, failure_count = ?2, last_failure = ?3
             WHERE id = ?4",
            params![new_status.as_str(), new_count, last_failure, id],
        )
        .map_err(|e| format!("fail_dispatch UPDATE: {e}"))?;

        // 回读最新行返回给调用方。
        let updated = tx
            .query_row(
                "SELECT id, task_id, assignee_handle, status, failure_count,
                        last_heartbeat_at, last_failure, dispatched_at, completed_at, created_at
                 FROM dispatch_contexts WHERE id = ?",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .map_err(|e| format!("fail_dispatch re-read: {e}"))?;
        Ok(Some(DispatchContext {
            id: updated.0,
            task_id: updated.1,
            assignee: updated.2,
            status: DispatchStatus::from_str(&updated.3)?,
            failure_count: updated.4 as u32,
            last_heartbeat_at: updated.5,
            last_failure: updated.6,
            dispatched_at: updated.7,
            completed_at: updated.8,
            created_at: updated.9,
        }))
    }
}

// 局部复用 database 模块的锁辅助宏，避免在文件顶部 `use` 一遍再破坏隔离。
macro_rules! lock_conn {
    ($mutex:expr) => {
        $mutex
            .lock()
            .map_err(|e| format!("Hermes Store mutex poisoned: {}", e))?
    };
}
use lock_conn;

// =============================================================================
// Tests —— TDD：先写失败测试，再补实现。覆盖 5 条并发不变量。
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hermes::types::RuntimeKind;

    fn sample_task(id: &str, deps: Vec<&str>) -> Task {
        Task {
            id: id.to_string(),
            parent_id: None,
            spec: format!("spec for {id}"),
            status: TaskStatus::Pending, // 会被 create_task 覆盖
            deps: deps.into_iter().map(String::from).collect(),
            result: None,
            assignment: None,
            created_at: "2026-06-28T00:00:00Z".to_string(),
            completed_at: None,
        }
    }

    fn sample_dispatch(id: &str, task_id: &str) -> DispatchContext {
        DispatchContext {
            id: id.to_string(),
            task_id: task_id.to_string(),
            assignee: Some("agent_1".to_string()),
            status: DispatchStatus::Dispatched,
            failure_count: 0,
            last_heartbeat_at: None,
            last_failure: None,
            dispatched_at: Some("2026-06-28T00:00:00Z".to_string()),
            completed_at: None,
            created_at: "2026-06-28T00:00:00Z".to_string(),
        }
    }

    /// 不变量 1：无 deps → Ready；list_tasks(ready=true) 能查到。
    #[test]
    fn task_with_no_deps_is_ready() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("t1", vec![])).unwrap();
        let got = store.get_task("t1").unwrap().expect("task present");
        assert_eq!(got.status, TaskStatus::Ready, "no-deps task should be Ready");
        assert!(got.deps.is_empty());
        let ready_list = store
            .list_tasks(TaskListFilter { ready: true, ..Default::default() })
            .unwrap();
        assert_eq!(ready_list.len(), 1, "list_tasks(ready) returns it");
        assert_eq!(ready_list[0].id, "t1");
    }

    /// 不变量 2：带 deps → Pending；依赖未完成时 promote 不提升它。
    #[test]
    fn task_with_uncompleted_deps_stays_pending() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("root", vec![])).unwrap();
        store
            .create_task(sample_task("child", vec!["root"]))
            .unwrap();

        let child = store.get_task("child").unwrap().unwrap();
        assert_eq!(child.status, TaskStatus::Pending, "child with deps starts Pending");

        // root 还没 completed，child 不应该出现在 ready 列表
        let ready = store
            .list_tasks(TaskListFilter { ready: true, ..Default::default() })
            .unwrap();
        assert!(
            !ready.iter().any(|t| t.id == "child"),
            "child must NOT be ready while its dep is not Completed"
        );
    }

    /// 不变量 3：依赖被标记 Completed 后，下游 Pending 在同一事务内变 Ready。
    #[test]
    fn completing_a_dep_promotes_dependent_in_same_transaction() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("root", vec![])).unwrap();
        store
            .create_task(sample_task("child", vec!["root"]))
            .unwrap();

        // 完成 root
        store
            .update_task_status("root", TaskStatus::Completed)
            .unwrap();

        // 立即读 child —— 必须 Ready（同事务提交）
        let child = store.get_task("child").unwrap().unwrap();
        assert_eq!(
            child.status,
            TaskStatus::Ready,
            "child must be promoted atomically after dep completes"
        );

        // 多 dep：全部完成才提升
        store.create_task(sample_task("root2", vec![])).unwrap();
        store
            .create_task(sample_task("multi", vec!["root", "root2"]))
            .unwrap();
        let multi = store.get_task("multi").unwrap().unwrap();
        assert_eq!(multi.status, TaskStatus::Pending, "multi has root2 outstanding");

        store
            .update_task_status("root2", TaskStatus::Completed)
            .unwrap();
        let multi = store.get_task("multi").unwrap().unwrap();
        assert_eq!(
            multi.status,
            TaskStatus::Ready,
            "multi promoted only after all deps complete"
        );
    }

    /// 不变量 4：dispatch 累计失败到 3 → CircuitBroken，且不再 dispatchable。
    #[test]
    fn dispatch_circuit_breaks_at_three_failures() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("t", vec![])).unwrap();
        store.create_dispatch(sample_dispatch("ctx", "t")).unwrap();

        // 第 1 次
        let after_1 = store.fail_dispatch("ctx", "err 1").unwrap().unwrap();
        assert_eq!(after_1.status, DispatchStatus::Failed, "1st failure → Failed");
        assert_eq!(after_1.failure_count, 1);
        assert!(after_1.status != DispatchStatus::CircuitBroken);

        // 第 2 次
        let after_2 = store.fail_dispatch("ctx", "err 2").unwrap().unwrap();
        assert_eq!(after_2.status, DispatchStatus::Failed, "2nd failure → Failed");
        assert_eq!(after_2.failure_count, 2);

        // 第 3 次 —— 熔断
        let after_3 = store.fail_dispatch("ctx", "err 3").unwrap().unwrap();
        assert_eq!(
            after_3.status,
            DispatchStatus::CircuitBroken,
            "3rd failure → CircuitBroken (熔断)"
        );
        assert_eq!(after_3.failure_count, 3);
        assert_eq!(
            after_3.last_failure.as_deref(),
            Some("err 3"),
            "last_failure updated"
        );

        // 不再 dispatchable：用 get_dispatch 确认状态在 DB 落地
        let persisted = store.get_dispatch("ctx").unwrap().unwrap();
        assert_eq!(persisted.status, DispatchStatus::CircuitBroken);
    }

    /// 不变量 5：Task 往返——写入再读出，所有字段（deps JSON、status、assignment）一致。
    #[test]
    fn task_round_trip_preserves_all_fields() {
        let store = Store::open_in_memory().unwrap();
        let mut t = sample_task("rt", vec!["dep_a", "dep_b"]);
        t.status = TaskStatus::Pending; // create_task 会按 deps 推导
        t.assignment = Some(AgentAssignment {
            runtime: RuntimeKind::Sdk,
            tool: "claude-sdk".to_string(),
            model: "sonnet".to_string(),
        });
        store.create_task(t.clone()).unwrap();

        let got = store.get_task("rt").unwrap().unwrap();
        assert_eq!(got.id, "rt");
        assert_eq!(got.parent_id, None);
        assert_eq!(got.spec, "spec for rt");
        assert_eq!(got.status, TaskStatus::Pending, "with deps → Pending");
        assert_eq!(got.deps, vec!["dep_a".to_string(), "dep_b".to_string()]);
        assert_eq!(got.result, None);
        assert_eq!(
            got.assignment,
            Some(AgentAssignment {
                runtime: RuntimeKind::Sdk,
                tool: "claude-sdk".to_string(),
                model: "sonnet".to_string(),
            }),
            "assignment JSON round-trips"
        );
        assert_eq!(got.created_at, "2026-06-28T00:00:00Z");
        assert_eq!(got.completed_at, None);
    }
}
