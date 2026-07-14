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
    AgentAssignment, DecisionGate, DispatchContext, DispatchStatus, GateStatus, Message,
    MessageType, RunStatus, CoordinatorRun, Task, TaskStatus,
};
use rusqlite::{params, Connection, Transaction};
use std::path::Path;
use std::sync::{Arc, Mutex};

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

/// 启动恢复时持久化到既有 task/dispatch 原因字段的事实性原因。
/// `coordinator_runs` 没有 error 列，因此 run 本身只记录 Failed；任务和派发的既有
/// result / last_failure 列保留可向用户展示的恢复原因。
const STARTUP_ORPHAN_RECOVERY_REASON: &str =
    "previous Hermes process ended before the coordinator completed";

// =============================================================================
// Store 结构
// =============================================================================

/// Hermes 编排状态机的 SQLite 句柄。所有写操作通过 `Mutex` 串行化；
/// 关键不变量（status 写 + ready 提升）在同一 `Transaction` 内提交。
pub struct Store {
    /// `Arc<Mutex<Connection>>`——`Arc` 让 Coordinator 把句柄 clone 进 watcher
    /// spawned future（'static）共享同一连接；`Mutex` 串行化所有写者。
    /// `Transaction` 由 `Connection::transaction` 创建后会借用 `&mut Connection`，
    /// 故仍需独占锁。
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerSession {
    pub dispatch_id: String, pub run_id: String, pub task_id: String, pub worker_id: String,
    pub final_response: Option<String>, pub error: Option<String>, pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerTranscriptEntry { pub kind: String, pub payload: String, pub created_at: String }

impl Store {
    /// 克隆内部 Arc 句柄——两个 clone 共享同一 SQLite 连接。
    /// Coordinator 用它把 Store 传给 spawned watcher（'static future）。
    pub fn clone_handle(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
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
            conn: Arc::new(Mutex::new(conn)),
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
            conn: Arc::new(Mutex::new(conn)),
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

        tx.execute_batch("CREATE TABLE IF NOT EXISTS worker_sessions (
            dispatch_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, task_id TEXT NOT NULL,
            worker_id TEXT NOT NULL, final_response TEXT, error TEXT,
            created_at TEXT NOT NULL, completed_at TEXT);
            CREATE INDEX IF NOT EXISTS idx_worker_sessions_run ON worker_sessions(run_id, created_at DESC);
            CREATE TABLE IF NOT EXISTS worker_transcript_entries (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT, dispatch_id TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('message_raw', 'activity')), payload TEXT NOT NULL,
            created_at TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS idx_worker_transcript_dispatch ON worker_transcript_entries(dispatch_id, sequence);")
            .map_err(|e| format!("Failed to create worker transcript tables: {e}"))?;

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
        // Phase 3c：CHECK 新增 'cancelled'（RunStatus::Cancelled）。Hermes DB 是 Phase 2
        // 新建、无历史数据，schema 直接改即可。若本地 dev 已有旧 schema 的 hermes.db，
        // CREATE TABLE IF NOT EXISTS 不会重建——需删库后重启 dev 让新 CHECK 生效。
        // （in-memory 测试 DB 每次重建，故测试不受影响。）
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS coordinator_runs (
                id                  TEXT PRIMARY KEY,
                spec                TEXT NOT NULL,
                status              TEXT NOT NULL DEFAULT 'idle'
                  CHECK(status IN ('idle', 'running', 'completed', 'failed', 'cancelled')),
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
        // M2: 状态过滤改为参数化绑定（params![s.as_str()]），与 ready 分支保持一致；
        // 同时杜绝把 enum token 字符串拼进 SQL 字面量的写法。
        let mut stmt = if filter.ready {
            conn.prepare(
                "SELECT id, parent_id, spec, status, deps, result, assignment,
                        created_at, completed_at
                 FROM tasks WHERE status = ?1 ORDER BY created_at",
            )
            .map_err(|e| format!("Failed to prepare list_tasks (ready): {e}"))?
        } else if let Some(s) = filter.status {
            conn.prepare(
                "SELECT id, parent_id, spec, status, deps, result, assignment,
                        created_at, completed_at
                 FROM tasks WHERE status = ?1 ORDER BY created_at",
            )
            .map_err(|e| format!("Failed to prepare list_tasks (status): {e}"))?
        } else {
            conn.prepare(
                "SELECT id, parent_id, spec, status, deps, result, assignment,
                        created_at, completed_at
                 FROM tasks ORDER BY created_at",
            )
            .map_err(|e| format!("Failed to prepare list_tasks (all): {e}"))?
        };

        let mut out = Vec::new();
        if filter.ready {
            let mut rows = stmt
                .query(params![TaskStatus::Ready.as_str()])
                .map_err(|e| format!("Failed to query list_tasks (ready): {e}"))?;
            while let Some(row) = rows
                .next()
                .map_err(|e| format!("Failed to fetch list_tasks row: {e}"))?
            {
                out.push(Self::row_to_task(row)?);
            }
        } else if let Some(s) = filter.status {
            let mut rows = stmt
                .query(params![s.as_str()])
                .map_err(|e| format!("Failed to query list_tasks (status): {e}"))?;
            while let Some(row) = rows
                .next()
                .map_err(|e| format!("Failed to fetch list_tasks row: {e}"))?
            {
                out.push(Self::row_to_task(row)?);
            }
        } else {
            let mut rows = stmt
                .query([])
                .map_err(|e| format!("Failed to query list_tasks (all): {e}"))?;
            while let Some(row) = rows
                .next()
                .map_err(|e| format!("Failed to fetch list_tasks row: {e}"))?
            {
                out.push(Self::row_to_task(row)?);
            }
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
    ///
    /// `result`：可选结果字符串。`Some(s)` 写入；`None` 用 `COALESCE` 保留既有值
    /// ——这样 Coordinator 可以在 worker_done 回填结果后，后续状态更新（如幂等重写）
    /// 不会清空已落库的 result（M1 修复，对齐 orca `updateTaskStatus(id, status, result?)`）。
    pub fn update_task_status(
        &self,
        id: &str,
        new_status: TaskStatus,
        result: Option<&str>,
    ) -> Result<(), String> {
        let mut conn = lock_conn!(self.conn);
        // 关键：status 写 + ready 提升放同一 tx，要么一起提交要么一起回滚。
        let tx = conn
            .transaction()
            .map_err(|e| format!("BEGIN update_task_status tx failed: {e}"))?;
        Self::update_task_status_in_tx(&tx, id, new_status, result)?;
        tx.commit()
            .map_err(|e| format!("COMMIT update_task_status tx failed: {e}"))?;
        Ok(())
    }

    fn update_task_status_in_tx(
        tx: &Transaction,
        id: &str,
        new_status: TaskStatus,
        result: Option<&str>,
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
                // M1: result 用 COALESCE(?, result) —— None 不覆盖既有值。
                // 因此 Coordinator 在 Retry 路径调用 update_task_status(id, Ready, None)
                // 时，先前 circuit-breaker 写入的失败 result 会被保留（Finding 2 验证）。
                "UPDATE tasks SET status = ?1,
                                      result = COALESCE(?2, result),
                                      completed_at = COALESCE(?3, completed_at)
                 WHERE id = ?4",
                params![new_status.as_str(), result, completed_at, id],
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

    /// 更新 task 的 assignment（Phase 2 Task 14 Finding 1：让 Reassign 真正生效）。
    ///
    /// 与 [`Store::create_task`] 写入 assignment 时一致——JSON TEXT 序列化。
    /// 不动 status / result / completed_at（reassign 的语义是「换兵」，状态变化由
    /// 调用方在前后用 [`Store::update_task_status`] 控制）。
    pub fn update_task_assignment(
        &self,
        task_id: &str,
        assignment: &AgentAssignment,
    ) -> Result<(), String> {
        let json = serde_json::to_string(assignment)
            .map_err(|e| format!("Failed to serialize assignment: {e}"))?;
        let conn = lock_conn!(self.conn);
        let updated = conn
            .execute(
                "UPDATE tasks SET assignment = ?1 WHERE id = ?2",
                params![json, task_id],
            )
            .map_err(|e| format!("UPDATE tasks.assignment failed: {e}"))?;
        if updated == 0 {
            return Err(format!("Task not found: {task_id}"));
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

    /// 列出心跳超时的派发上下文（Task 10：stale-dispatch reaping）。
    ///
    /// 语义：`status='dispatched'` 且 `last_heartbeat_at` 早于 `now - threshold_secs`。
    ///
    /// 实现选择（在 Rust 里比较而非纯 SQL `datetime` 比较）：
    /// - `last_heartbeat_at` 由 [`DispatchContext`] 写入时统一为 RFC-3339（`Utc::now::to_rfc3339`），
    ///   是字典序可比较的 ISO-8601；但 SQLite `datetime()` 只识别 `YYYY-MM-DDTHH:MM:SS`
    ///   （无时区/小数秒），用 SQL 直接比较会丢精度。这里先 SELECT 出全部
    ///   `dispatched` 上下文，再用 `chrono::DateTime` 精确比较——更鲁棒、无格式坑。
    /// - threshold 取 `u64` 秒，避免上层魔法串。
    pub fn get_stale_dispatches(&self, threshold_secs: u64) -> Result<Vec<DispatchContext>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, assignee_handle, status, failure_count,
                        last_heartbeat_at, last_failure, dispatched_at, completed_at, created_at
                 FROM dispatch_contexts
                 WHERE status = ?1",
            )
            .map_err(|e| format!("prepare get_stale_dispatches: {e}"))?;
        let mut rows = stmt
            .query(params![DispatchStatus::Dispatched.as_str()])
            .map_err(|e| format!("query get_stale_dispatches: {e}"))?;
        let mut out = Vec::new();
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(threshold_secs as i64);
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("fetch get_stale_dispatches row: {e}"))?
        {
            let ctx = Self::row_to_dispatch(row)?;
            // 缺心跳（NULL）一律视为 stale——派发后从未上报心跳本身就是异常。
            let hb_str = match ctx.last_heartbeat_at.as_deref() {
                Some(s) => s,
                None => {
                    out.push(ctx);
                    continue;
                }
            };
            let hb = match chrono::DateTime::parse_from_rfc3339(hb_str) {
                Ok(t) => t.with_timezone(&chrono::Utc),
                // 格式异常：保守起见也算 stale（让上层回收）。
                Err(_) => {
                    out.push(ctx);
                    continue;
                }
            };
            if hb < cutoff {
                out.push(ctx);
            }
        }
        Ok(out)
    }

    /// 列出所有当前状态为 `Dispatched` 的派发上下文（Task 18：supervisor-in-loop）。
    ///
    /// 用途：Coordinator 的 `tick` 在 supervisor 标记 Suspect agent 后，需要找到
    /// 这些 agent 对应的 active dispatch 行（拿到 dispatch_id / task_id）才能
    /// `fail_dispatch` + abort runtime。Supervisor 只返回 agent_id 列表，这层
    /// "agent_id → active dispatch" 的映射由本方法提供。
    ///
    /// 实现：`status = 'dispatched'` 过滤即可——dispatch 表的唯一活跃态就是
    /// Dispatched（Failed / CircuitBroken 是终态/熔断态，不会被 Suspect 命中）。
    /// 返回顺序按 `created_at`（与 list_tasks 一致，便于确定性测试）。
    pub fn list_active_dispatches(&self) -> Result<Vec<DispatchContext>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, assignee_handle, status, failure_count,
                        last_heartbeat_at, last_failure, dispatched_at, completed_at, created_at
                 FROM dispatch_contexts
                 WHERE status = ?1
                 ORDER BY created_at",
            )
            .map_err(|e| format!("prepare list_active_dispatches: {e}"))?;
        let mut rows = stmt
            .query(params![DispatchStatus::Dispatched.as_str()])
            .map_err(|e| format!("query list_active_dispatches: {e}"))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("fetch list_active_dispatches row: {e}"))?
        {
            out.push(Self::row_to_dispatch(row)?);
        }
        Ok(out)
    }

    /// **测试专用**：把指定 dispatch 的 `last_heartbeat_at` 强制改成给定时间，
    /// 用于构造 stale 派发场景（生产代码不应调用）。`#[cfg(test)]` 守卫避免泄漏。
    #[cfg(test)]
    pub fn set_dispatch_heartbeat_for_test(
        &self,
        dispatch_id: &str,
        heartbeat: &str,
    ) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        let updated = conn
            .execute(
                "UPDATE dispatch_contexts SET last_heartbeat_at = ?1 WHERE id = ?2",
                params![heartbeat, dispatch_id],
            )
            .map_err(|e| format!("set_dispatch_heartbeat_for_test UPDATE: {e}"))?;
        if updated == 0 {
            return Err(format!("dispatch not found: {dispatch_id}"));
        }
        Ok(())
    }

    /// **测试专用**：按 task_id 找它当前 `dispatched` 状态的最新派发 id。
    /// 用于 stale-reap 测试定位 Coordinator 自动生成的 dispatch（id 不可预测）。
    #[cfg(test)]
    pub fn find_active_dispatch_id_for_test(&self, task_id: &str) -> Result<String, String> {
        let conn = lock_conn!(self.conn);
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM dispatch_contexts
                 WHERE task_id = ?1 AND status = ?2
                 ORDER BY rowid DESC LIMIT 1",
                params![task_id, DispatchStatus::Dispatched.as_str()],
                |r| r.get::<_, String>(0),
            )
            .ok();
        id.ok_or_else(|| format!("no active dispatch for task {task_id}"))
    }

    /// 取该 task 历史 dispatch 中的最大 `failure_count`。
    ///
    /// 用途：Coordinator 在重派（task 退回 Ready 后再次 dispatch_one）时，把上次的
    /// 失败计数 carry-forward 到新 dispatch（对齐 orca `createDispatchContext` 的
    /// `MAX(failure_count)` 语义）。这样跨多次 dispatch 的失败仍能累计到熔断阈值。
    /// 若该 task 无历史 dispatch，返回 0。
    pub fn latest_failure_count_for_task(&self, task_id: &str) -> Result<u32, String> {
        let conn = lock_conn!(self.conn);
        let max_count: Option<i64> = conn
            .query_row(
                "SELECT MAX(failure_count) FROM dispatch_contexts WHERE task_id = ?1",
                params![task_id],
                |r| r.get(0),
            )
            .ok();
        Ok(max_count.unwrap_or(0) as u32)
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

    // ── Message bus ──────────────────────────────────────────────────────

    /// 写入一条消息。`sequence` 由 SQLite `rowid AUTOINCREMENT` 单调分配：
    /// 选用 AUTOINCREMENT（而非 `MAX(sequence)+1`）是因为它在并发 / 回滚场景下
    /// 由 SQLite 内部 `sqlite_sequence` 表维护，**绝不复用**已分配的 rowid——
    /// 即使事务回滚也不会回退序列号。这样多个 worker_done / heartbeat 并发
    /// 落库时，消息顺序严格反映"先到先服务"，便于 Coordinator 按 sequence 顺序
    /// 消费 inbox（移植 orca `insertMessage` line 287）。
    ///
    /// 调用方提供的 `msg.sequence` 会被覆盖；返回的 Message 已带真实 sequence。
    pub fn insert_message(&self, msg: Message) -> Result<Message, String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO messages (
                id, from_handle, to_handle, subject, body, type, priority,
                thread_id, payload, read, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                msg.id,
                msg.from,
                msg.to,
                msg.subject,
                msg.body,
                msg.kind.as_str(),
                msg.priority,
                msg.thread_id,
                msg.payload,
                msg.read as i64,
                msg.created_at,
            ],
        )
        .map_err(|e| format!("Failed to insert message: {e}"))?;

        // 回读 sequence（由 AUTOINCREMENT 分配），便于调用方拿到权威值。
        // query_row 的 closure 在锁内执行，可直接调用 row_to_message 构造 Message。
        let stored = conn
            .query_row(
                "SELECT id, from_handle, to_handle, subject, body, type, priority,
                        thread_id, payload, read, sequence, created_at
                 FROM messages WHERE id = ?",
                params![msg.id],
                |row| {
                    // 在闭包内只做无 fallible 转换的部分；枚举 from_str 在外层做。
                    Ok(MessageRowLite {
                        id: row.get(0)?,
                        from: row.get(1)?,
                        to: row.get(2)?,
                        subject: row.get(3)?,
                        body: row.get(4)?,
                        type_str: row.get(5)?,
                        priority: row.get(6)?,
                        thread_id: row.get(7).ok(),
                        payload: row.get(8).ok(),
                        read_i: row.get(9)?,
                        sequence: row.get::<_, i64>(10)? as u64,
                        created_at: row.get(11)?,
                    })
                },
            )
            .map_err(|e| format!("Failed to re-read message: {e}"))?;
        Ok(Message {
            id: stored.id,
            from: stored.from,
            to: stored.to,
            subject: stored.subject,
            body: stored.body,
            kind: MessageType::from_str(&stored.type_str)?,
            priority: stored.priority,
            thread_id: stored.thread_id,
            payload: stored.payload,
            read: stored.read_i != 0,
            sequence: stored.sequence,
            created_at: stored.created_at,
        })
    }

    /// 按 `to_handle` 列出 inbox。`unread_only=true` 只返回未读；按 `sequence` 升序。
    /// 对齐 orca `getUnreadMessages`（line 305）—— Coordinator 按 sequence 顺序消费，
    /// 保证先到先处理。
    pub fn list_inbox(&self, to_handle: &str, opts: InboxFilter) -> Result<Vec<Message>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = if opts.unread_only {
            conn.prepare(
                "SELECT id, from_handle, to_handle, subject, body, type, priority,
                        thread_id, payload, read, sequence, created_at
                 FROM messages WHERE to_handle = ?1 AND read = 0
                 ORDER BY sequence ASC",
            )
            .map_err(|e| format!("Failed to prepare list_inbox (unread): {e}"))?
        } else {
            conn.prepare(
                "SELECT id, from_handle, to_handle, subject, body, type, priority,
                        thread_id, payload, read, sequence, created_at
                 FROM messages WHERE to_handle = ?1
                 ORDER BY sequence ASC",
            )
            .map_err(|e| format!("Failed to prepare list_inbox (all): {e}"))?
        };
        let mut rows = stmt
            .query(params![to_handle])
            .map_err(|e| format!("Failed to query list_inbox: {e}"))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("Failed to fetch list_inbox row: {e}"))?
        {
            out.push(Self::row_to_message(row)?);
        }
        Ok(out)
    }

    /// 按消息 sequence（自增主键）批量标记已读。对齐 orca `markAsRead(ids)` line 367，
    /// 区别在于 orca 用字符串 id，这里用 sequence（主键，更稳定且高效）。
    pub fn mark_read_by_ids(&self, sequences: &[i64]) -> Result<(), String> {
        if sequences.is_empty() {
            return Ok(());
        }
        let conn = lock_conn!(self.conn);
        let placeholders = std::iter::repeat("?")
            .take(sequences.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("UPDATE messages SET read = 1 WHERE sequence IN ({placeholders})");
        conn.execute(
            &sql,
            rusqlite::params_from_iter(sequences.iter().copied()),
        )
        .map_err(|e| format!("Failed to mark_read_by_ids: {e}"))?;
        Ok(())
    }

    /// 按 `to_handle` 标记全部未读为已读（批量场景）。Coordinator 在消费完 inbox
    /// 一轮后通常调用此方法清理收件箱。
    pub fn mark_read_by_handle(&self, to_handle: &str) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE messages SET read = 1 WHERE to_handle = ?1 AND read = 0",
            params![to_handle],
        )
        .map_err(|e| format!("Failed to mark_read_by_handle: {e}"))?;
        Ok(())
    }

    fn row_to_message(row: &rusqlite::Row) -> Result<Message, String> {
        let id: String = row.get(0).map_err(|e| format!("message.id: {e}"))?;
        let from: String = row.get(1).map_err(|e| format!("message.from: {e}"))?;
        let to: String = row.get(2).map_err(|e| format!("message.to: {e}"))?;
        let subject: String = row.get(3).map_err(|e| format!("message.subject: {e}"))?;
        let body: String = row.get(4).map_err(|e| format!("message.body: {e}"))?;
        let type_str: String = row.get(5).map_err(|e| format!("message.type: {e}"))?;
        let priority: String = row.get(6).map_err(|e| format!("message.priority: {e}"))?;
        let thread_id: Option<String> = row.get(7).ok();
        let payload: Option<String> = row.get(8).ok();
        let read_i: i64 = row.get(9).map_err(|e| format!("message.read: {e}"))?;
        let sequence: u64 = row
            .get::<_, i64>(10)
            .map_err(|e| format!("message.sequence: {e}"))?
            as u64;
        let created_at: String = row.get(11).map_err(|e| format!("message.created_at: {e}"))?;

        Ok(Message {
            id,
            from,
            to,
            subject,
            body,
            kind: MessageType::from_str(&type_str)?,
            priority,
            thread_id,
            payload,
            read: read_i != 0,
            sequence,
            created_at,
        })
    }

    // ── Decision gates ──────────────────────────────────────────────────
    //
    // Gate 状态流：Pending → Resolved（外部回答）/ Timeout。
    // - create_gate：插入 Pending gate，并把对应 task 置为 Blocked（等待人工决策）。
    //   （对齐 orca `createGate` line 735。当前实现不级联 task 状态——YAGNI，
    //   由调用方 Coordinator 决定是否 Block 任务。仅持久化 gate 记录。）
    // - resolve_gate：写入 resolution + resolved_at，状态 → Resolved。
    //   对齐 orca `resolveGate` line 748（不在此自动 unblock task，留给 Coordinator）。

    pub fn create_gate(
        &self,
        task_id: &str,
        question: &str,
        options: Vec<String>,
    ) -> Result<DecisionGate, String> {
        let id = format!("gate_{}", uuid_v4_short());
        let options_json = serde_json::to_string(&options)
            .map_err(|e| format!("Failed to serialize gate options: {e}"))?;
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO decision_gates (id, task_id, question, options, status)
             VALUES (?, ?, ?, ?, ?)",
            params![id, task_id, question, options_json, GateStatus::Pending.as_str()],
        )
        .map_err(|e| format!("Failed to insert gate: {e}"))?;

        Ok(DecisionGate {
            id,
            task_id: task_id.to_string(),
            question: question.to_string(),
            options,
            resolution: None,
            status: GateStatus::Pending,
        })
    }

    pub fn resolve_gate(&self, gate_id: &str, resolution: String) -> Result<(), String> {
        // Task 4 (D.5)：resolved_at 改用 chrono RFC3339 绑定（替代 datetime('now')）。
        let now = chrono::Utc::now().to_rfc3339();
        let conn = lock_conn!(self.conn);
        let updated = conn
            .execute(
                "UPDATE decision_gates
                 SET status = ?1, resolution = ?2, resolved_at = ?3
                 WHERE id = ?4",
                params![GateStatus::Resolved.as_str(), resolution, now, gate_id],
            )
            .map_err(|e| format!("Failed to resolve gate: {e}"))?;
        if updated == 0 {
            return Err(format!("Gate not found: {gate_id}"));
        }
        Ok(())
    }

    pub fn list_gates(&self, filter: GateListFilter) -> Result<Vec<DecisionGate>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = match (filter.task_id.as_deref(), filter.status) {
            (Some(_), Some(_)) => conn
                .prepare(
                    "SELECT id, task_id, question, options, status, resolution
                     FROM decision_gates WHERE task_id = ?1 AND status = ?2
                     ORDER BY created_at",
                )
                .map_err(|e| format!("prepare list_gates(task+status): {e}"))?,
            (Some(_), None) => conn
                .prepare(
                    "SELECT id, task_id, question, options, status, resolution
                     FROM decision_gates WHERE task_id = ?1
                     ORDER BY created_at",
                )
                .map_err(|e| format!("prepare list_gates(task): {e}"))?,
            (None, Some(_)) => conn
                .prepare(
                    "SELECT id, task_id, question, options, status, resolution
                     FROM decision_gates WHERE status = ?1
                     ORDER BY created_at",
                )
                .map_err(|e| format!("prepare list_gates(status): {e}"))?,
            (None, None) => conn
                .prepare(
                    "SELECT id, task_id, question, options, status, resolution
                     FROM decision_gates ORDER BY created_at",
                )
                .map_err(|e| format!("prepare list_gates(all): {e}"))?,
        };

        let mut rows = match (filter.task_id.as_deref(), filter.status) {
            (Some(t), Some(s)) => stmt.query(params![t, s.as_str()]),
            (Some(t), None) => stmt.query(params![t]),
            (None, Some(s)) => stmt.query(params![s.as_str()]),
            (None, None) => stmt.query([]),
        }
        .map_err(|e| format!("Failed to query list_gates: {e}"))?;

        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| format!("Failed to fetch list_gates row: {e}"))?
        {
            out.push(Self::row_to_gate(row)?);
        }
        Ok(out)
    }

    fn row_to_gate(row: &rusqlite::Row) -> Result<DecisionGate, String> {
        let id: String = row.get(0).map_err(|e| format!("gate.id: {e}"))?;
        let task_id: String = row.get(1).map_err(|e| format!("gate.task_id: {e}"))?;
        let question: String = row.get(2).map_err(|e| format!("gate.question: {e}"))?;
        let options_str: String = row.get(3).map_err(|e| format!("gate.options: {e}"))?;
        let status_str: String = row.get(4).map_err(|e| format!("gate.status: {e}"))?;
        let resolution: Option<String> = row.get(5).ok();

        let options: Vec<String> = serde_json::from_str(&options_str)
            .map_err(|e| format!("Failed to deserialize gate options '{options_str}': {e}"))?;

        Ok(DecisionGate {
            id,
            task_id,
            question,
            options,
            resolution,
            status: GateStatus::from_str(&status_str)?,
        })
    }

    // ── Durable worker transcript ──────────────────────────────────────

    pub fn create_worker_session(&self, dispatch_id: &str, run_id: &str, task_id: &str, worker_id: &str) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute("INSERT INTO worker_sessions (dispatch_id, run_id, task_id, worker_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![dispatch_id, run_id, task_id, worker_id, chrono::Utc::now().to_rfc3339()])
            .map_err(|e| format!("Failed to create worker session: {e}"))?;
        Ok(())
    }

    pub fn append_worker_transcript_entry(&self, dispatch_id: &str, kind: &str, payload: &str) -> Result<(), String> {
        if !matches!(kind, "message_raw" | "activity") { return Err(format!("Invalid transcript entry kind: {kind}")); }
        let conn = lock_conn!(self.conn);
        conn.execute("INSERT INTO worker_transcript_entries (dispatch_id, kind, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![dispatch_id, kind, payload, chrono::Utc::now().to_rfc3339()])
            .map_err(|e| format!("Failed to append worker transcript entry: {e}"))?;
        Ok(())
    }

    pub fn complete_worker_session(&self, dispatch_id: &str, final_response: Option<&str>, error: Option<&str>) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        let updated = conn.execute("UPDATE worker_sessions SET final_response = ?1, error = ?2, completed_at = ?3 WHERE dispatch_id = ?4",
            params![final_response, error, chrono::Utc::now().to_rfc3339(), dispatch_id])
            .map_err(|e| format!("Failed to complete worker session: {e}"))?;
        if updated == 0 { return Err(format!("Worker session not found: {dispatch_id}")); }
        Ok(())
    }

    /// User cancellation is not a worker failure: it must never consume circuit-breaker budget.
    pub fn abort_dispatch(&self, dispatch_id: &str, reason: &str) -> Result<Option<DispatchContext>, String> {
        let conn = lock_conn!(self.conn);
        let updated = conn.execute("UPDATE dispatch_contexts SET status = ?1, last_failure = ?2, completed_at = ?3 WHERE id = ?4 AND status = ?5",
            params![DispatchStatus::Failed.as_str(), reason, chrono::Utc::now().to_rfc3339(), dispatch_id, DispatchStatus::Dispatched.as_str()])
            .map_err(|e| format!("Failed to abort dispatch: {e}"))?;
        if updated == 0 { return Ok(None); }
        let mut stmt = conn.prepare("SELECT id, task_id, assignee_handle, status, failure_count, last_heartbeat_at, last_failure, dispatched_at, completed_at, created_at FROM dispatch_contexts WHERE id = ?1").map_err(|e| format!("prepare aborted dispatch: {e}"))?;
        let row = stmt.query_row(params![dispatch_id], |row| Ok(DispatchContext { id: row.get(0)?, task_id: row.get(1)?, assignee: row.get(2)?, status: DispatchStatus::from_str(&row.get::<_, String>(3)?).map_err(|_| rusqlite::Error::InvalidQuery)?, failure_count: row.get::<_, i64>(4)? as u32, last_heartbeat_at: row.get(5)?, last_failure: row.get(6)?, dispatched_at: row.get(7)?, completed_at: row.get(8)?, created_at: row.get(9)? })).map_err(|e| format!("read aborted dispatch: {e}"))?;
        Ok(Some(row))
    }

    pub fn list_worker_sessions(&self, run_id: Option<&str>) -> Result<Vec<WorkerSession>, String> {
        let conn = lock_conn!(self.conn);
        let sql = if run_id.is_some() { "SELECT dispatch_id, run_id, task_id, worker_id, final_response, error, created_at, completed_at FROM worker_sessions WHERE run_id = ?1 ORDER BY created_at DESC" } else { "SELECT dispatch_id, run_id, task_id, worker_id, final_response, error, created_at, completed_at FROM worker_sessions ORDER BY created_at DESC" };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare list worker sessions: {e}"))?;
        let mut rows = if let Some(id) = run_id { stmt.query(params![id]) } else { stmt.query([]) }.map_err(|e| format!("query list worker sessions: {e}"))?;
        let mut sessions = Vec::new();
        while let Some(row) = rows.next().map_err(|e| format!("read worker session: {e}"))? {
            sessions.push(WorkerSession { dispatch_id: row.get(0).map_err(|e| e.to_string())?, run_id: row.get(1).map_err(|e| e.to_string())?, task_id: row.get(2).map_err(|e| e.to_string())?, worker_id: row.get(3).map_err(|e| e.to_string())?, final_response: row.get(4).ok(), error: row.get(5).ok(), created_at: row.get(6).map_err(|e| e.to_string())?, completed_at: row.get(7).ok() });
        }
        Ok(sessions)
    }

    pub fn list_worker_transcript_entries(&self, dispatch_id: &str) -> Result<Vec<WorkerTranscriptEntry>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn.prepare("SELECT kind, payload, created_at FROM worker_transcript_entries WHERE dispatch_id = ?1 ORDER BY sequence ASC").map_err(|e| format!("prepare worker transcript: {e}"))?;
        let mut rows = stmt.query(params![dispatch_id]).map_err(|e| format!("query worker transcript: {e}"))?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().map_err(|e| format!("read worker transcript: {e}"))? { entries.push(WorkerTranscriptEntry { kind: row.get(0).map_err(|e| e.to_string())?, payload: row.get(1).map_err(|e| e.to_string())?, created_at: row.get(2).map_err(|e| e.to_string())? }); }
        Ok(entries)
    }

    // ── Coordinator runs ────────────────────────────────────────────────

    /// 开启一次 Coordinator 编排运行。状态置为 `Running`（对齐 orca `createCoordinatorRun`）。
    pub fn create_run(
        &self,
        goal: &str,
        coordinator_handle: &str,
        poll_interval_ms: u64,
    ) -> Result<CoordinatorRun, String> {
        // Task 4 (D.5)：显式绑定 RFC3339 created_at，让 DB 行与返回 struct 共用同一个
        // `now`（此前 DB 走 schema DEFAULT datetime('now')，struct 走 chrono——格式不一致）。
        let now = chrono::Utc::now().to_rfc3339();
        let id = format!("run_{}", uuid_v4_short());
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to BEGIN create coordinator_run tx: {e}"))?;

        match tx.query_row(
            "SELECT id FROM coordinator_runs WHERE status = ?1 ORDER BY created_at DESC LIMIT 1",
            params![RunStatus::Running.as_str()],
            |row| row.get::<_, String>(0),
        ) {
            Ok(active_run_id) => {
                return Err(format!(
                    "Cannot start a new Hermes run while run {active_run_id} is still active"
                ));
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(e) => return Err(format!("Failed to check active coordinator_run: {e}")),
        }

        // Task / dispatch rows are global operational state, not run history. Once no run is
        // active, terminal rows from the prior run must not suppress the next planner pass.
        tx.execute(
            "DELETE FROM dispatch_contexts WHERE status IN (?1, ?2, ?3)",
            params![
                DispatchStatus::Completed.as_str(),
                DispatchStatus::Failed.as_str(),
                DispatchStatus::CircuitBroken.as_str(),
            ],
        )
        .map_err(|e| format!("Failed to clear terminal dispatch state: {e}"))?;
        tx.execute(
            "DELETE FROM tasks WHERE status IN (?1, ?2, ?3)",
            params![
                TaskStatus::Completed.as_str(),
                TaskStatus::Failed.as_str(),
                TaskStatus::Blocked.as_str(),
            ],
        )
        .map_err(|e| format!("Failed to clear terminal task state: {e}"))?;

        tx.execute(
            "INSERT INTO coordinator_runs
                (id, spec, status, coordinator_handle, poll_interval_ms, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                id,
                goal,
                RunStatus::Running.as_str(),
                coordinator_handle,
                poll_interval_ms as i64,
                now,
            ],
        )
        .map_err(|e| format!("Failed to insert coordinator_run: {e}"))?;
        tx.commit()
            .map_err(|e| format!("Failed to COMMIT create coordinator_run tx: {e}"))?;

        Ok(CoordinatorRun {
            id,
            goal: goal.to_string(),
            status: RunStatus::Running,
            coordinator_handle: coordinator_handle.to_string(),
            poll_interval_ms,
            created_at: now,
            completed_at: None,
        })
    }

    /// Returns the most recent persisted reason for a terminal task failure. The task result is
    /// the authoritative root cause; a terminal dispatch reason is retained as a fallback.
    pub fn terminal_failure_reason(&self) -> Result<Option<String>, String> {
        let conn = lock_conn!(self.conn);
        match conn.query_row(
            "SELECT result
             FROM tasks
             WHERE status = ?1 AND result IS NOT NULL AND trim(result) <> ''
             ORDER BY completed_at DESC, created_at DESC
             LIMIT 1",
            params![TaskStatus::Failed.as_str()],
            |row| row.get::<_, String>(0),
        ) {
            Ok(reason) => return Ok(Some(reason)),
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(e) => return Err(format!("Failed to read failed task reason: {e}")),
        }

        match conn.query_row(
            "SELECT last_failure
             FROM dispatch_contexts
             WHERE status IN (?1, ?2) AND last_failure IS NOT NULL AND trim(last_failure) <> ''
             ORDER BY completed_at DESC, created_at DESC
             LIMIT 1",
            params![
                DispatchStatus::Failed.as_str(),
                DispatchStatus::CircuitBroken.as_str(),
            ],
            |row| row.get::<_, String>(0),
        ) {
            Ok(reason) => Ok(Some(reason)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to read failed dispatch reason: {e}")),
        }
    }

    pub fn update_run(
        &self,
        run_id: &str,
        status: RunStatus,
    ) -> Result<(), String> {
        // Task 4 (D.5)：用 chrono RFC3339 绑定替代 SQLite `datetime('now')`，
        // 让所有 runtime 写入的时间戳格式统一（与 create_run / create_dispatch 等
        // 已用 chrono 的路径一致）。CASE 表达式保持原语义：终态时写 now，否则 NULL
        // → COALESCE 保留既有值。
        // Phase 3c：'cancelled' 也是终态，需记 completed_at（cancel 时 run 结束）。
        let now = chrono::Utc::now().to_rfc3339();
        let conn = lock_conn!(self.conn);
        let updated = conn
            .execute(
                "UPDATE coordinator_runs
                 SET status = ?1,
                     completed_at = COALESCE(
                        CASE WHEN ?1 IN ('completed', 'failed', 'cancelled') THEN ?3 ELSE NULL END,
                        completed_at
                     )
                 WHERE id = ?2",
                params![status.as_str(), run_id, now],
            )
            .map_err(|e| format!("Failed to update coordinator_run: {e}"))?;
        if updated == 0 {
            return Err(format!("Run not found: {run_id}"));
        }
        Ok(())
    }

    pub fn get_active_run(&self) -> Result<Option<CoordinatorRun>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, spec, status, coordinator_handle, poll_interval_ms,
                        created_at, completed_at
                 FROM coordinator_runs
                 WHERE status = ?1
                 ORDER BY created_at DESC LIMIT 1",
            )
            .map_err(|e| format!("prepare get_active_run: {e}"))?;
        let mut rows = stmt
            .query(params![RunStatus::Running.as_str()])
            .map_err(|e| format!("query get_active_run: {e}"))?;
        match rows
            .next()
            .map_err(|e| format!("fetch get_active_run: {e}"))?
        {
            Some(row) => Ok(Some(Self::row_to_run(row)?)),
            None => Ok(None),
        }
    }

    /// Task 4：按 id 取一条 coordinator run（任意状态）。[`crate::hermes::HermesEngine::show_run`] 用。
    ///
    /// 与 [`Store::get_active_run`] 同构，去掉 status 过滤、按 id 精确取一行。
    /// 不存在 → Ok(None)。
    pub fn get_run(&self, run_id: &str) -> Result<Option<CoordinatorRun>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, spec, status, coordinator_handle, poll_interval_ms,
                        created_at, completed_at
                 FROM coordinator_runs
                 WHERE id = ?1",
            )
            .map_err(|e| format!("prepare get_run: {e}"))?;
        let mut rows = stmt
            .query(params![run_id])
            .map_err(|e| format!("query get_run: {e}"))?;
        match rows
            .next()
            .map_err(|e| format!("fetch get_run: {e}"))?
        {
            Some(row) => Ok(Some(Self::row_to_run(row)?)),
            None => Ok(None),
        }
    }

    fn row_to_run(row: &rusqlite::Row) -> Result<CoordinatorRun, String> {
        let id: String = row.get(0).map_err(|e| format!("run.id: {e}"))?;
        let goal: String = row.get(1).map_err(|e| format!("run.spec: {e}"))?;
        let status_str: String = row.get(2).map_err(|e| format!("run.status: {e}"))?;
        let coordinator_handle: String = row
            .get(3)
            .map_err(|e| format!("run.coordinator_handle: {e}"))?;
        let poll_interval_ms: u64 = row
            .get::<_, i64>(4)
            .map_err(|e| format!("run.poll_interval_ms: {e}"))?
            as u64;
        let created_at: String = row.get(5).map_err(|e| format!("run.created_at: {e}"))?;
        let completed_at: Option<String> = row.get(6).ok();

        Ok(CoordinatorRun {
            id,
            goal,
            status: RunStatus::from_str(&status_str)?,
            coordinator_handle,
            poll_interval_ms,
            created_at,
            completed_at,
        })
    }

    // ── Crash-recovery reconciliation ───────────────────────────────────
    //
    // 进程崩溃后，可能在 `dispatched` 状态留下"孤儿"任务。重启时 reconcile
    // 把这些任务收敛回正确状态（移植 orca `lifecycle-reconciliation`）：
    //
    // - 若已收到该 dispatch 对应的 worker_done（且 dispatch_id + task_id +
    //   assignee 三元组匹配，避免重试产生的陈旧 worker_done 错配）→
    //   把 task 标为 Completed 并持久化 worker_done payload 中的 result，
    //   dispatch 标为 Completed。
    // - 否则（dispatched 但无匹配 worker_done）→ agent 大概率崩溃/丢失，
    //   把 task 重置为 Ready（不是 Pending——下游已提升过；Ready 让
    //   Coordinator 下一轮重新派发），dispatch 标为 Failed。
    //
    // 整个扫描在同一事务内完成，保证一致性（中途崩溃不会留下半成品状态）。

    pub fn reconcile_on_startup(&self) -> Result<ReconcileReport, String> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| format!("BEGIN reconcile tx: {e}"))?;
        let report = Self::reconcile_in_tx(&tx)?;
        tx.commit().map_err(|e| format!("COMMIT reconcile tx: {e}"))?;
        Ok(report)
    }

    fn reconcile_in_tx(tx: &Transaction) -> Result<ReconcileReport, String> {
        let mut completed_via_worker_done: u64 = 0;
        let mut marked_for_redispatch: u64 = 0;

        // This method is called only during application startup, before this process has built
        // any RunHandle. A persisted Running row is therefore an orphan from a previous process.
        // Tasks and dispatches are global operational state with no run_id, so once such a row is
        // found every non-terminal task/dispatch belongs to that interrupted operational epoch.
        let has_orphaned_run: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM coordinator_runs WHERE status = ?1)",
                params![RunStatus::Running.as_str()],
                |row| row.get(0),
            )
            .map_err(|e| format!("query orphaned coordinator_run: {e}"))?;

        // 1) 收集所有"孤儿 dispatched"任务 + 它们当前活跃的 dispatch 上下文。
        //    一条 task 理论上同时只有一个活跃 dispatch；取最新的那条（对齐 orca
        //    `getDispatchContext` ORDER BY rowid DESC LIMIT 1）。
        let mut stmt = tx
            .prepare(
                "SELECT t.id, t.status
                 FROM tasks t
                 WHERE t.status = ?1",
            )
            .map_err(|e| format!("prepare reconcile scan: {e}"))?;
        let orphan_task_ids: Vec<String> = stmt
            .query_map(params![TaskStatus::Dispatched.as_str()], |r| r.get::<_, String>(0))
            .map_err(|e| format!("query reconcile scan: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect reconcile scan: {e}"))?;
        drop(stmt);

        for task_id in orphan_task_ids {
            // 取该 task 当前活跃的 dispatch（dispatched 状态，最新一条）
            let active_dispatch: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT id, assignee_handle FROM dispatch_contexts
                     WHERE task_id = ?1 AND status = ?2
                     ORDER BY rowid DESC LIMIT 1",
                    params![task_id, DispatchStatus::Dispatched.as_str()],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
                )
                .ok();

            let Some((dispatch_id, dispatch_assignee)) = active_dispatch else {
                // 没有 dispatched 状态的上下文：说明任务状态与 dispatch 表不一致。
                // 视作无 worker_done 也能匹配的情况——直接把 task 推回 Ready
                // 让 Coordinator 自检（保守做法，避免卡死）。
                Self::reset_task_to_ready_in_tx(tx, &task_id)?;
                marked_for_redispatch += 1;
                continue;
            };

            // 2) 在 messages 表里查找匹配三元组的 worker_done（dispatch_id + task_id + from_handle=assignee）。
            //    对齐 orca `reconcileWorkerDoneMessage`：忽略陈旧/错配的 worker_done。
            let matched_msg: Option<(Option<String>,)> = tx
                .query_row(
                    "SELECT m.payload FROM messages m
                     WHERE m.type = ?1
                       AND m.from_handle = ?2
                       AND m.payload IS NOT NULL
                       AND json_extract(m.payload, '$.taskId') = ?3
                       AND json_extract(m.payload, '$.dispatchId') = ?4
                     ORDER BY m.sequence DESC LIMIT 1",
                    params![
                        MessageType::WorkerDone.as_str(),
                        dispatch_assignee.as_deref().unwrap_or(""),
                        task_id,
                        dispatch_id,
                    ],
                    |r| Ok((r.get::<_, Option<String>>(0)?,)),
                )
                .ok();

            if let Some((Some(payload),)) = matched_msg {
                // 回填：task → Completed（带 result），dispatch → Completed
                let result_text = extract_result_from_payload(&payload);
                Self::update_task_status_in_tx(
                    tx,
                    &task_id,
                    TaskStatus::Completed,
                    result_text.as_deref(),
                )?;
                // Task 4 (D.5)：completed_at 改用 chrono RFC3339 绑定（替代 datetime('now')）。
                let now = chrono::Utc::now().to_rfc3339();
                tx.execute(
                    "UPDATE dispatch_contexts
                     SET status = ?1, completed_at = ?2
                     WHERE id = ?3",
                    params![DispatchStatus::Completed.as_str(), now, dispatch_id],
                )
                .map_err(|e| format!("reconcile complete dispatch: {e}"))?;
                completed_via_worker_done += 1;
            } else {
                // 重派：task → Ready，dispatch → Failed
                Self::reset_task_to_ready_in_tx(tx, &task_id)?;
                tx.execute(
                    "UPDATE dispatch_contexts
                     SET status = ?1
                     WHERE id = ?2",
                    params![DispatchStatus::Failed.as_str(), dispatch_id],
                )
                .map_err(|e| format!("reconcile fail dispatch: {e}"))?;
                marked_for_redispatch += 1;
            }
        }

        if has_orphaned_run {
            // Preserve the existing dispatched-task reconciliation above so a persisted
            // worker_done is still recorded as Completed. Then converge all remaining
            // non-terminal operational rows before the next create_run transaction clears its
            // terminal state and starts the planner from empty state.
            let now = chrono::Utc::now().to_rfc3339();
            tx.execute(
                "UPDATE coordinator_runs
                 SET status = ?1, completed_at = COALESCE(completed_at, ?2)
                 WHERE status = ?3",
                params![
                    RunStatus::Failed.as_str(),
                    now,
                    RunStatus::Running.as_str(),
                ],
            )
            .map_err(|e| format!("fail orphaned coordinator_run: {e}"))?;
            tx.execute(
                "UPDATE tasks
                 SET status = ?1,
                     result = COALESCE(NULLIF(result, ''), ?2),
                     completed_at = COALESCE(completed_at, ?3)
                 WHERE status IN (?4, ?5, ?6)",
                params![
                    TaskStatus::Failed.as_str(),
                    STARTUP_ORPHAN_RECOVERY_REASON,
                    now,
                    TaskStatus::Pending.as_str(),
                    TaskStatus::Ready.as_str(),
                    TaskStatus::Dispatched.as_str(),
                ],
            )
            .map_err(|e| format!("fail orphaned task state: {e}"))?;
            tx.execute(
                "UPDATE dispatch_contexts
                 SET status = ?1,
                     last_failure = COALESCE(NULLIF(last_failure, ''), ?2),
                     completed_at = COALESCE(completed_at, ?3)
                 WHERE status IN (?4, ?5)",
                params![
                    DispatchStatus::Failed.as_str(),
                    STARTUP_ORPHAN_RECOVERY_REASON,
                    now,
                    DispatchStatus::Pending.as_str(),
                    DispatchStatus::Dispatched.as_str(),
                ],
            )
            .map_err(|e| format!("fail orphaned dispatch state: {e}"))?;
        }

        Ok(ReconcileReport {
            completed_via_worker_done,
            marked_for_redispatch,
        })
    }

    /// 把 task 重置回 Ready（而非 Pending）。
    /// 关键：deps 早已提升完毕（task 才能走到 dispatched），所以重置为 Ready 让
    /// Coordinator 下一轮重新派发即可；不需要重跑 promote（对齐 orca `failDispatch`
    /// line 723-725 的注释："set back to 'ready' (not 'pending')"）。
    /// 同时把 completed_at 清空，让 task 重新"活着"。
    fn reset_task_to_ready_in_tx(tx: &Transaction, task_id: &str) -> Result<(), String> {
        tx.execute(
            "UPDATE tasks SET status = ?1, completed_at = NULL WHERE id = ?2",
            params![TaskStatus::Ready.as_str(), task_id],
        )
        .map_err(|e| format!("reset_task_to_ready UPDATE: {e}"))?;
        Ok(())
    }
}

/// `insert_message` 回读时用的中间结构——rusqlite closure 不能返回 fallible
/// `Message`（`MessageType::from_str` 会失败），先存原始字符串再外层转换。
struct MessageRowLite {
    id: String,
    from: String,
    to: String,
    subject: String,
    body: String,
    type_str: String,
    priority: String,
    thread_id: Option<String>,
    payload: Option<String>,
    read_i: i64,
    sequence: u64,
    created_at: String,
}

/// `list_inbox` 的过滤参数（对齐 orca `getUnreadMessages(opts?)`）。
#[derive(Debug, Clone, Copy, Default)]
pub struct InboxFilter {
    /// true：仅返回 `read=0` 的消息。
    pub unread_only: bool,
}

/// `list_gates` 的过滤参数（对齐 orca `listGates(filter)`）。
#[derive(Debug, Clone, Default)]
pub struct GateListFilter {
    pub task_id: Option<String>,
    pub status: Option<GateStatus>,
}

/// `reconcile_on_startup` 的返回报告。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReconcileReport {
    /// 通过匹配 worker_done 消息回填为 Completed 的任务数。
    pub completed_via_worker_done: u64,
    /// 因无 worker_done 而被重置为 Ready、对应 dispatch 标 Failed 的任务数。
    pub marked_for_redispatch: u64,
}

/// 从 worker_done 消息 payload 中提取 result 文本。
/// payload 形如 `{"taskId": "...", "dispatchId": "...", "result": "..."}`。
/// 若 payload 缺 result 字段，返回 None（task 仍标 Completed，只是不带 result）。
fn extract_result_from_payload(payload: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    match v.get("result")? {
        serde_json::Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// 生成一个简短的伪 UUID（用于 gate / run id 前缀）。
/// 不依赖 `uuid` crate（YAGNI——非安全敏感场景，timestamp + 计数即可）。
fn uuid_v4_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
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
    use crate::hermes::types::{
        DecisionGate, GateStatus, Message, MessageType, RunStatus, RuntimeKind,
    };

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
            .update_task_status("root", TaskStatus::Completed, None)
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
            .update_task_status("root2", TaskStatus::Completed, None)
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
            provider: "claude".to_string(),
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
                provider: "claude".to_string(),
                tool: "claude-sdk".to_string(),
                model: "sonnet".to_string(),
            }),
            "assignment JSON round-trips"
        );
        assert_eq!(got.created_at, "2026-06-28T00:00:00Z");
        assert_eq!(got.completed_at, None);
    }

    #[test]
    fn task_round_trip_deserializes_legacy_assignment_without_provider() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("legacy-assignment", vec![])).unwrap();

        let legacy_json = r#"{"runtime":"Sdk","tool":"claude-sdk","model":"sonnet"}"#;
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET assignment = ?1 WHERE id = ?2",
            params![legacy_json, "legacy-assignment"],
        )
        .unwrap();
        drop(conn);

        let task = store
            .get_task("legacy-assignment")
            .unwrap()
            .expect("legacy task is present");
        let assignment = task.assignment.expect("legacy assignment is present");
        assert_eq!(assignment.provider, "claude");
        assert!(
            serde_json::to_string(&assignment)
                .unwrap()
                .contains(r#""provider":"claude""#),
            "the round-tripped assignment must carry the compatibility default"
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // Task 7 —— Message bus / Gates / Runs / Reconcile（先 RED，后 GREEN）
    // ──────────────────────────────────────────────────────────────────

    fn sample_message(seq_id: &str, to: &str, kind: MessageType) -> Message {
        Message {
            id: seq_id.to_string(),
            from: "coord".to_string(),
            to: to.to_string(),
            subject: format!("subj {seq_id}"),
            body: String::new(),
            kind,
            priority: "normal".to_string(),
            thread_id: None,
            payload: None,
            read: false,
            // 由 insert_message 覆盖；测试构造时随意给个占位
            sequence: 0,
            created_at: "2026-06-28T00:00:00Z".to_string(),
        }
    }

    /// T1: 同一 to_handle 的 3 条消息按 sequence 升序返回。
    #[test]
    fn inbox_returns_messages_in_ascending_sequence() {
        let store = Store::open_in_memory().unwrap();
        let to = "agent_1";
        store.insert_message(sample_message("m1", to, MessageType::Status)).unwrap();
        store.insert_message(sample_message("m2", to, MessageType::Status)).unwrap();
        store.insert_message(sample_message("m3", to, MessageType::Status)).unwrap();

        let inbox = store
            .list_inbox(to, InboxFilter { unread_only: false })
            .unwrap();
        assert_eq!(inbox.len(), 3, "3 messages to same handle");
        // sequence 严格单调递增
        assert!(inbox[0].sequence < inbox[1].sequence);
        assert!(inbox[1].sequence < inbox[2].sequence);
        // 按插入顺序 m1 < m2 < m3
        assert_eq!(inbox[0].id, "m1");
        assert_eq!(inbox[1].id, "m2");
        assert_eq!(inbox[2].id, "m3");
    }

    /// T2: unread_only 只返回未读；mark_read 把它们翻成已读。
    #[test]
    fn inbox_unread_filter_and_mark_read() {
        let store = Store::open_in_memory().unwrap();
        let to = "agent_1";
        store.insert_message(sample_message("m1", to, MessageType::Status)).unwrap();
        store.insert_message(sample_message("m2", to, MessageType::Status)).unwrap();

        // 初始两条都未读
        let unread = store
            .list_inbox(to, InboxFilter { unread_only: true })
            .unwrap();
        assert_eq!(unread.len(), 2, "both unread initially");

        // mark_read by to_handle：翻成已读
        store.mark_read_by_handle(to).unwrap();
        let unread_after = store
            .list_inbox(to, InboxFilter { unread_only: true })
            .unwrap();
        assert_eq!(unread_after.len(), 0, "no unread after mark_read");

        // unread_only=false 仍能看到全部（已读 + 未读）
        let all = store
            .list_inbox(to, InboxFilter { unread_only: false })
            .unwrap();
        assert_eq!(all.len(), 2);

        // mark_read by ids 也工作：新插一条，按 id 翻
        store.insert_message(sample_message("m3", to, MessageType::Status)).unwrap();
        let m3 = store
            .list_inbox(to, InboxFilter { unread_only: true })
            .unwrap();
        assert_eq!(m3.len(), 1);
        store.mark_read_by_ids(&[m3[0].sequence as i64]).unwrap();
        let final_unread = store
            .list_inbox(to, InboxFilter { unread_only: true })
            .unwrap();
        assert_eq!(final_unread.len(), 0);
    }

    /// T3: Gate 生命周期——create → list(Pending) 命中 → resolve → list(Resolved) 命中。
    #[test]
    fn gate_lifecycle_create_resolve_list() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("gt", vec![])).unwrap();

        let gate = store
            .create_gate("gt", "继续吗？", vec!["yes".to_string(), "no".to_string()])
            .unwrap();
        assert_eq!(gate.status, GateStatus::Pending);
        assert_eq!(gate.options, vec!["yes".to_string(), "no".to_string()]);

        // Pending 列表命中
        let pending = store
            .list_gates(GateListFilter {
                task_id: None,
                status: Some(GateStatus::Pending),
            })
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, gate.id);

        // resolve
        store.resolve_gate(&gate.id, "yes".to_string()).unwrap();

        // Pending 列表空，Resolved 命中
        let pending_after = store
            .list_gates(GateListFilter {
                task_id: None,
                status: Some(GateStatus::Pending),
            })
            .unwrap();
        assert_eq!(pending_after.len(), 0, "no pending after resolve");

        let resolved = store
            .list_gates(GateListFilter {
                task_id: None,
                status: Some(GateStatus::Resolved),
            })
            .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, gate.id);
        assert_eq!(resolved[0].resolution.as_deref(), Some("yes"));
    }

    /// T4: Reconcile —— dispatched 任务有 worker_done → 完成。
    #[test]
    fn reconcile_worker_done_completes_task() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("rt", vec![])).unwrap();
        // 模拟 Coordinator 派发：建 dispatched 上下文 + 把任务推到 Dispatched
        let mut ctx = sample_dispatch("ctx_1", "rt");
        ctx.status = DispatchStatus::Dispatched;
        ctx.assignee = Some("worker_a".to_string());
        store.create_dispatch(ctx.clone()).unwrap();
        store
            .update_task_status("rt", TaskStatus::Dispatched, None)
            .unwrap();

        // worker_a 发回 worker_done 消息（payload 含 taskId/dispatchId/assignee）
        let mut msg = sample_message("wd", "coordinator", MessageType::WorkerDone);
        msg.from = "worker_a".to_string();
        msg.payload = Some(
            serde_json::json!({
                "taskId": "rt",
                "dispatchId": "ctx_1",
                "result": "all good",
            })
            .to_string(),
        );
        store.insert_message(msg).unwrap();

        let report = store.reconcile_on_startup().unwrap();
        assert_eq!(report.completed_via_worker_done, 1, "1 task completed via worker_done");
        assert_eq!(report.marked_for_redispatch, 0);

        // task 现在是 Completed，且 result 被回填
        let t = store.get_task("rt").unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert!(t.result.is_some(), "result populated from worker_done payload");

        // dispatch 也变 Completed
        let d = store.get_dispatch("ctx_1").unwrap().unwrap();
        assert_eq!(d.status, DispatchStatus::Completed);
    }

    /// T5: Reconcile —— dispatched 任务无 worker_done → 标记重派（task → Ready, dispatch → Failed）。
    #[test]
    fn reconcile_missing_worker_done_redispatches() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("rt2", vec![])).unwrap();
        let mut ctx = sample_dispatch("ctx_2", "rt2");
        ctx.status = DispatchStatus::Dispatched;
        ctx.assignee = Some("worker_b".to_string());
        store.create_dispatch(ctx.clone()).unwrap();
        store
            .update_task_status("rt2", TaskStatus::Dispatched, None)
            .unwrap();
        // 不插 worker_done 消息——模拟 worker 崩溃

        let report = store.reconcile_on_startup().unwrap();
        assert_eq!(report.completed_via_worker_done, 0);
        assert_eq!(report.marked_for_redispatch, 1, "1 task marked for redispatch");

        let t = store.get_task("rt2").unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::Ready, "task reset to Ready for re-dispatch");

        let d = store.get_dispatch("ctx_2").unwrap().unwrap();
        assert_eq!(d.status, DispatchStatus::Failed, "stale dispatch marked Failed");
    }

    #[test]
    fn reconcile_startup_converges_an_orphan_run_before_a_new_run() {
        let store = Store::open_in_memory().unwrap();
        let orphan = store
            .create_run("interrupted goal", "coordinator", 5)
            .expect("create run that represents the previous process");

        store
            .create_task(sample_task("ready-task", vec![]))
            .expect("create ready task");
        store
            .create_task(sample_task("dispatched-task", vec![]))
            .expect("create dispatched task");
        store
            .update_task_status("dispatched-task", TaskStatus::Dispatched, None)
            .expect("mark task dispatched");
        store
            .create_task(sample_task("pending-task", vec!["ready-task"]))
            .expect("create pending task");

        let mut active_dispatch = sample_dispatch("active-dispatch", "dispatched-task");
        active_dispatch.status = DispatchStatus::Dispatched;
        store
            .create_dispatch(active_dispatch)
            .expect("create dispatched context");
        let mut pending_dispatch = sample_dispatch("pending-dispatch", "ready-task");
        pending_dispatch.status = DispatchStatus::Pending;
        store
            .create_dispatch(pending_dispatch)
            .expect("create pending context");

        let report = store
            .reconcile_on_startup()
            .expect("reconcile interrupted process state");

        assert_eq!(
            store.get_run(&orphan.id).unwrap().unwrap().status,
            RunStatus::Failed,
            "the persisted running run must become terminal at startup"
        );
        for task_id in ["ready-task", "dispatched-task", "pending-task"] {
            let task = store.get_task(task_id).unwrap().unwrap();
            assert_eq!(task.status, TaskStatus::Failed, "{task_id} must converge");
            assert!(
                task.result.as_deref().unwrap_or_default().contains("previous Hermes process"),
                "{task_id} must retain the factual recovery reason"
            );
        }
        for dispatch_id in ["active-dispatch", "pending-dispatch"] {
            let dispatch = store.get_dispatch(dispatch_id).unwrap().unwrap();
            assert_eq!(
                dispatch.status,
                DispatchStatus::Failed,
                "{dispatch_id} must converge before cleanup"
            );
        }
        assert_eq!(report.marked_for_redispatch, 1);

        let next = store
            .create_run("fresh goal", "coordinator", 5)
            .expect("an orphaned run must not block the next run");
        assert_eq!(next.status, RunStatus::Running);
        assert!(
            store.list_tasks(TaskListFilter::default()).unwrap().is_empty(),
            "the new run must start with an empty planner state"
        );
        assert!(
            store.get_dispatch("active-dispatch").unwrap().is_none(),
            "terminal operational state is cleared only by create_run"
        );
        assert_eq!(
            store.get_run(&orphan.id).unwrap().unwrap().status,
            RunStatus::Failed,
            "run history is retained after starting the next run"
        );
    }

    /// T6 (M1 回归): update_task_status 携带 result 时被持久化。
    #[test]
    fn update_task_status_persists_result() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("rr", vec![])).unwrap();
        store
            .update_task_status("rr", TaskStatus::Completed, Some("result-text"))
            .unwrap();
        let got = store.get_task("rr").unwrap().unwrap();
        assert_eq!(got.status, TaskStatus::Completed);
        assert_eq!(got.result.as_deref(), Some("result-text"), "result persisted");

        // 不传 result 不覆盖既有 result（COALESCE 语义）
        store
            .update_task_status("rr", TaskStatus::Completed, None)
            .unwrap();
        let got2 = store.get_task("rr").unwrap().unwrap();
        assert_eq!(
            got2.result.as_deref(),
            Some("result-text"),
            "None must not clobber existing result"
        );
    }

    /// T7 (Task 14 Finding 1): update_task_assignment 把新 assignment 写入 task。
    #[test]
    fn update_task_assignment_overwrites() {
        let store = Store::open_in_memory().unwrap();
        // 用带初始 assignment 的 task。
        let mut t = sample_task("asg", vec![]);
        t.assignment = Some(AgentAssignment {
            runtime: RuntimeKind::Sdk,
            provider: "claude".to_string(),
            tool: "claude-sdk".to_string(),
            model: "sonnet".to_string(),
        });
        store.create_task(t).unwrap();

        // 回读确认初始值。
        let before = store.get_task("asg").unwrap().unwrap();
        assert_eq!(before.assignment.as_ref().unwrap().model, "sonnet");

        // 换兵：runtime=cli, model=glm-5.2。
        let new_assignment = AgentAssignment {
            runtime: RuntimeKind::Cli,
            provider: "codex".to_string(),
            tool: "claude-cli".to_string(),
            model: "glm-5.2".to_string(),
        };
        store.update_task_assignment("asg", &new_assignment).unwrap();

        let after = store.get_task("asg").unwrap().unwrap();
        let a = after.assignment.expect("assignment present after update");
        assert_eq!(a.runtime, RuntimeKind::Cli);
        assert_eq!(a.tool, "claude-cli");
        assert_eq!(a.model, "glm-5.2");

        // 找不到 task → Err。
        let err = store.update_task_assignment("missing", &new_assignment);
        assert!(err.is_err(), "update_task_assignment on missing task errors");
    }

    /// T8 (Task 14 Finding 2 回归): Retry 的 update_task_status(Ready, None)
    /// 不清空先前 circuit-breaker 写入的失败 result——SQL 用 COALESCE 保留。
    #[test]
    fn retry_preserves_circuit_broken_result() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("cb", vec![])).unwrap();

        // 模拟失败 → Failed + 失败 result。
        store
            .update_task_status("cb", TaskStatus::Failed, Some("circuit broken: 3 fails"))
            .unwrap();
        let failed = store.get_task("cb").unwrap().unwrap();
        assert_eq!(failed.status, TaskStatus::Failed);
        assert_eq!(failed.result.as_deref(), Some("circuit broken: 3 fails"));

        // Coordinator Retry 路径：update_task_status(Ready, None)。
        store
            .update_task_status("cb", TaskStatus::Ready, None)
            .unwrap();
        let retried = store.get_task("cb").unwrap().unwrap();
        assert_eq!(retried.status, TaskStatus::Ready);
        assert_eq!(
            retried.result.as_deref(),
            Some("circuit broken: 3 fails"),
            "Retry(Ready, None) 必须保留失败的 result（COALESCE 语义，Finding 2）"
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // Task 4 —— get_run（任意状态）+ RFC3339 时间戳（D.5）
    // ──────────────────────────────────────────────────────────────────

    /// Task 4：get_run 按 id 精确取 run，不带 status 过滤——Completed 的 run
    /// 仍可被 get_run 取回（对比 get_active_run 在 run 进入终态后返回 None）。
    #[test]
    fn get_run_returns_any_status() {
        let store = Store::open_in_memory().unwrap();
        let run = store
            .create_run("do something", "coordinator", 1000)
            .unwrap();
        // 此时 get_active_run 命中（status = Running）。
        assert!(store.get_active_run().unwrap().is_some());

        // 推到终态。
        store.update_run(&run.id, RunStatus::Completed).unwrap();
        // get_active_run 不再命中（status != Running）。
        assert!(
            store.get_active_run().unwrap().is_none(),
            "Completed run 不应再被 get_active_run 命中"
        );
        // get_run 仍能按 id 取回（任意状态）。
        let got = store
            .get_run(&run.id)
            .unwrap()
            .expect("get_run 应返回 Completed 的 run");
        assert_eq!(got.id, run.id);
        assert_eq!(got.status, RunStatus::Completed, "状态回读正确");
        // Task 4 (D.5)：completed_at 应为 RFC3339（非 SQLite datetime('now') 格式）。
        // chrono::Utc::now().to_rfc3339() 产出形如 "2026-06-28T08:22:05+00:00"，
        // 含 'T' 且以 RFC3339 时区后缀（'Z' 或 '+00:00'）结尾。
        let completed = got
            .completed_at
            .as_deref()
            .expect("Completed run 必须有 completed_at");
        assert!(
            is_rfc3339_chrono(completed),
            "completed_at 必须是 RFC3339（chrono 格式，含 T + 时区后缀），实际值：{completed}"
        );
    }

    /// Task 4 (D.5)：create_run 返回的 created_at 与 DB 行的 created_at 一致
    /// （此前 DB 走 datetime('now')、struct 走 chrono——格式不同）。
    #[test]
    fn create_run_struct_and_db_row_agree_on_created_at() {
        let store = Store::open_in_memory().unwrap();
        let run = store.create_run("g", "coordinator", 500).unwrap();
        // 返回 struct 的 created_at 必须是 RFC3339（chrono 格式）。
        assert!(
            is_rfc3339_chrono(&run.created_at),
            "struct.created_at 必须是 RFC3339（chrono 格式），实际：{}",
            run.created_at
        );
        // DB 行的 created_at 必须与 struct 一致（同一值，不再分歧）。
        let db_run = store.get_run(&run.id).unwrap().unwrap();
        assert_eq!(
            db_run.created_at, run.created_at,
            "DB 行与返回 struct 的 created_at 必须一致（Task 4 D.5 修复）"
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // Task 10（3c）—— RunStatus::Cancelled 落库：CHECK 放行 + completed_at 终态
    // ──────────────────────────────────────────────────────────────────

    /// Task 10（3c）：update_run(id, Cancelled) 必须被 schema CHECK 放行，
    /// 且 cancelled 是终态 → completed_at 必须被写入。
    /// （若 CHECK 未加 'cancelled'，update_run 会返回 SQLite CHECK constraint failed。）
    #[test]
    fn update_run_accepts_cancelled_status() {
        let store = Store::open_in_memory().unwrap();
        let run = store.create_run("cancel me", "coord", 5).unwrap();
        store.update_run(&run.id, RunStatus::Cancelled).unwrap();
        let got = store.get_run(&run.id).unwrap().expect("run present");
        assert_eq!(got.status, RunStatus::Cancelled, "CHECK 必须放行 cancelled");
        assert!(
            got.completed_at.is_some(),
            "cancelled 是终态，应记 completed_at"
        );
    }

    /// Task 4 (D.5)：resolve_gate 的 resolved_at 落库为 RFC3339（非 datetime('now')）。
    #[test]
    fn resolve_gate_writes_rfc3339_resolved_at() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("rg", vec![])).unwrap();
        let gate = store.create_gate("rg", "Q?", vec!["A".into()]).unwrap();
        store.resolve_gate(&gate.id, "A".into()).unwrap();

        // 直接 SQL 回读 resolved_at（绕过 row_to_gate——它没暴露 resolved_at 字段）。
        let conn = store.conn.lock().expect("store conn lock");
        let resolved_at: Option<String> = conn
            .query_row(
                "SELECT resolved_at FROM decision_gates WHERE id = ?1",
                params![gate.id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        drop(conn);
        let resolved_at = resolved_at.expect("resolved_at 必须落库");
        assert!(
            is_rfc3339_chrono(&resolved_at),
            "resolved_at 必须是 RFC3339（chrono 格式），实际：{resolved_at}"
        );
    }

    #[test]
    fn create_run_rejects_an_overlapping_active_run() {
        let store = Store::open_in_memory().unwrap();
        let active = store.create_run("first goal", "coordinator", 5).unwrap();

        let err = store
            .create_run("second goal", "coordinator", 5)
            .expect_err("a second run must not start while another run is active");

        assert!(
            err.contains(&active.id),
            "overlap error must identify the active run, got: {err}"
        );
    }

    #[test]
    fn create_run_clears_terminal_operational_state_but_preserves_run_history() {
        let store = Store::open_in_memory().unwrap();
        let previous = store.create_run("failed goal", "coordinator", 5).unwrap();
        store
            .create_task(sample_task("t1", vec![]))
            .expect("create terminal task");
        store
            .update_task_status("t1", TaskStatus::Failed, Some("worker exited with code 1"))
            .expect("mark task failed");
        let mut failed_dispatch = sample_dispatch("disp_t1", "t1");
        failed_dispatch.status = DispatchStatus::CircuitBroken;
        failed_dispatch.last_failure = Some("worker exited with code 1".to_string());
        store
            .create_dispatch(failed_dispatch)
            .expect("create terminal dispatch");
        store
            .update_run(&previous.id, RunStatus::Failed)
            .expect("mark previous run failed");

        let next = store
            .create_run("next goal", "coordinator", 5)
            .expect("terminal operational state must not block a new run");

        assert_eq!(next.status, RunStatus::Running);
        assert!(
            store
                .list_tasks(TaskListFilter::default())
                .expect("list tasks")
                .is_empty(),
            "terminal tasks must not leak into the next planner run"
        );
        assert!(
            store
                .get_dispatch("disp_t1")
                .expect("get dispatch")
                .is_none(),
            "terminal dispatches must not leak into the next planner run"
        );
        assert_eq!(
            store
                .get_run(&previous.id)
                .expect("get previous run")
                .expect("previous run history")
                .status,
            RunStatus::Failed,
            "coordinator run history must be retained"
        );
    }

    #[test]
    fn worker_transcript_entries_survive_terminal_run_and_operational_cleanup() {
        let store = Store::open_in_memory().unwrap();
        let run = store.create_run("history goal", "coordinator", 5).unwrap();

        store.create_worker_session("disp-1", &run.id, "task-1", "worker-1").unwrap();
        store.append_worker_transcript_entry(
            "disp-1",
            "message_raw",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"real response"}]}}"#,
        ).unwrap();
        store.complete_worker_session("disp-1", Some("real response"), None).unwrap();
        store.update_run(&run.id, RunStatus::Completed).unwrap();
        store.create_run("next goal", "coordinator", 5).unwrap();

        let sessions = store.list_worker_sessions(Some(&run.id)).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].final_response.as_deref(), Some("real response"));
        let entries = store.list_worker_transcript_entries("disp-1").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "message_raw");
    }

    #[test]
    fn terminal_failure_reason_reads_the_failed_task_result() {
        let store = Store::open_in_memory().unwrap();
        store.create_task(sample_task("failed-task", vec![])).unwrap();
        store
            .update_task_status(
                "failed-task",
                TaskStatus::Failed,
                Some("worker exited with code 1"),
            )
            .unwrap();

        assert_eq!(
            store.terminal_failure_reason().unwrap(),
            Some("worker exited with code 1".to_string())
        );
    }

    /// 判断时间戳字符串是否为 chrono 的 RFC3339 输出（含 'T' 且以 'Z' 或 '+HH:MM' 结尾）。
    /// 用来区分 `chrono::Utc::now().to_rfc3339()`（如 `2026-06-28T08:22:05+00:00`）
    /// 与 SQLite `datetime('now')`（如 `2026-06-28 08:22:05`——空格分隔、无时区）。
    fn is_rfc3339_chrono(s: &str) -> bool {
        s.contains('T')
            && (s.ends_with('Z') || {
                // 形如 +HH:MM / -HH:MM 的时区后缀。
                let tail = if s.len() >= 6 { &s[s.len() - 6..] } else { return false; };
                matches!(tail.as_bytes()[0], b'+' | b'-')
                    && tail.as_bytes()[3] == b':'
            })
    }
}
