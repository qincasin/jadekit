//! `CliRuntime` —— 把裸 CLI 进程（claude/gemini/codex CLI）包成 [`AgentRuntime`]。
//!
//! 与 [`crate::hermes::sdk_runtime::SdkRuntime`] 并列的第二个 [`AgentRuntime`]
//! 实现，证明 Hermes 引擎的「介质可插拔」契约。`SdkRuntime` 走 ai-bridge daemon
//! 的结构化 `StreamLine` 协议（`structured_events = true`）；本运行时直接驱动
//! 一个裸 CLI 进程，只有 stdout 文本 + 退出码，因此 `structured_events = false`，
//! 对应 [`crate::hermes::supervisor::WorkerSupervisor`] 的**降级 liveness 档**：
//! 即使 agent 一直在吐文本，沉默超时也会被判为存活，但绝对硬超时仍会兜底。
//!
//! PTY spawn / read / kill 流程：
//! 1. [`AgentRuntime::start`]：调用方给的命令（`command: Vec<String>`，第一段是
//!    可执行文件名，其余为参数）+ `spec.cwd`，通过 `portable_pty` 打开 PTY pair，
//!    在 slave 侧 spawn 子进程。slave 立即 drop（RFC 1857 drop order）。master
//!    保留进 [`CliSession`]，后续每次 [`AgentRuntime::send`] 都 `try_clone_reader`
//!    一份给读任务，`take_writer` 只在 start 调一次（API 约束：writer 只能取一次）。
//! 2. [`AgentRuntime::send`]：把已取出的 writer 写 `prompt + "\n"`（PTY stdin），
//!    spawn 一个 **`spawn_blocking`** 读任务阻塞读 master reader，按 `\n` 切行，
//!    每行（去 `\r`、去尾换行）映射成 [`AgentEvent::TextDelta`]。reader EOF 后，
//!    读任务再 spawn 一个 `tokio::spawn` 任务调 `child.wait()` 拿 exit status，
//!    按 `exit.success()` 发 [`AgentEvent::Done`]（success=退出码==0）。
//! 3. [`AgentRuntime::abort`] / [`AgentRuntime::stop`]：用 `clone_killer()` 派生
//!    的 killer 发 `kill()` 信号（Unix 下 portable-pty 先 SIGHUP + 兜底 SIGKILL，
//!    Windows 下走 TerminateProcess）。PTY slave 子进程是 session leader，SIGHUP
//!    到达后会扩散到同 session 的全部子进程，达到「进程组级终止」的效果。
//! 4. [`AgentRuntime::liveness`]：`child.try_wait()` 返回 `None` → `Alive`；
//!    返回 `Some(_)` → `Dead`；会话不存在或 wait 出错 → `Dead`（容错）。
//!
//! 设计取舍：PTY 的 master reader 是 `Box<dyn Read + Send>`（阻塞），不能在
//! async 任务里 `await`；用 `spawn_blocking` 把它放回阻塞线程池，再用 channel
//! 把行数据回流到 async 世界，是与 tokio 集成的标准做法。child 的 `wait()` 同理
//! 也是阻塞，故放在 reader EOF 之后的独立 `tokio::spawn`（不再 spawn_blocking
//! 是因为 `wait` 通常很快，且我们要在 wait 返回后立刻 channel.send）。

use async_trait::async_trait;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use super::runtime::{
    AgentEvent, AgentHandle, AgentRuntime, Liveness, RuntimeCapabilities, RuntimeError,
    RuntimeStartSpec,
};

/// 单个 CLI agent 的运行时会话：持有 PTY 子进程的全部句柄。
///
/// 所有字段都是 `Send`：`CliRuntime` 把每个会话放进 `Arc<Mutex<HashMap>>`，
/// 跨 `AgentRuntime` 方法共享。
struct CliSession {
    /// PTY master：用于每次 send 时 `try_clone_reader` 一份给读任务。
    /// 注意 `take_writer` 只能调用一次，故 writer 在 start 时就取出存进 session。
    master: Box<dyn portable_pty::MasterPty + Send>,
    /// PTY 的写端：`send` 写 `prompt + "\n"` 进去（→ 子进程 stdin）。
    writer: Option<Box<dyn Write + Send>>,
    /// 子进程句柄：`liveness` 用 `try_wait` 探活，`stop` 时 `wait` 收尸。
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// 派生出来的 killer：`abort` 用它发信号而不抢占 `child` 的 wait。
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// 一旦子进程已退出（被 abort/stop 或自然退出），置为 true；
    /// 防止 abort 后再次 kill 已死进程时报错。
    killed: bool,
    /// 当前活跃的 reader task。每次新 send 前会 await 上一次的（若有），
    /// 防止多个并发读任务交错写同一 channel。CLI agent 一次只处理一条 prompt，
    /// 串行 send 是合理约束。
    reader_task: Option<JoinHandle<()>>,
}

/// 裸 CLI 进程适配器：Hermes 引擎通过 [`AgentRuntime`] 驱动它。
///
/// `command` 由调用方注入（生产侧用 `["claude"].to_vec()`，测试侧用
/// `["bash", "-c", "echo hi; exit 0"]`）——本运行时不内置任何 provider 名，
/// 避免魔法字符串。
pub struct CliRuntime {
    command: Vec<String>,
    sessions: Arc<Mutex<HashMap<String, CliSession>>>,
}

impl CliRuntime {
    /// 用给定命令构造运行时。每次 [`AgentRuntime::start`] 都会 spawn 一份该命令
    /// 的副本（一个 agent 一个进程）。
    pub fn new(command: Vec<String>) -> Self {
        Self {
            command,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl AgentRuntime for CliRuntime {
    fn capabilities(&self) -> RuntimeCapabilities {
        // 裸 CLI 只有文本 + 进程存活，没有结构化 tool_use/tool_result 事件 →
        // structured_events=false（WorkerSupervisor 降级 liveness 档）。
        // supports_resume=false：CLI 进程无 resume 协议。
        // supports_permission_prompt=false：无结构化权限事件（兼容未来扩展）。
        RuntimeCapabilities {
            structured_events: false,
            supports_resume: false,
            supports_permission_prompt: false,
        }
    }

    async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError> {
        if self.command.is_empty() {
            return Err(RuntimeError("CliRuntime::command 为空".to_string()));
        }

        // 1. 构造命令：第一段是可执行文件，其余为参数。
        let mut cmd_builder = CommandBuilder::new(&self.command[0]);
        for arg in &self.command[1..] {
            cmd_builder.arg(arg);
        }
        cmd_builder.cwd(&spec.cwd);

        // 2. 打开 PTY pair。尺寸用合理默认（24x80），与终端差异由后续 resize 处理。
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError(format!("openpty 失败: {e}")))?;

        // 3. 在 PTY slave 端 spawn 子进程。slave 释放后子进程仍是 session leader。
        let child = pair
            .slave
            .spawn_command(cmd_builder)
            .map_err(|e| RuntimeError(format!("spawn_command 失败: {e}")))?;
        // slave 端 spawn 完即 drop：RFC 1857 要求 slave 先于 master drop。
        drop(pair.slave);

        // 4. 派生 killer（独立于 child 的 wait，可从其它任务发信号）。
        let killer = child.clone_killer();

        // 5. 取出 master 的写端（API 约束：只能调一次，故存进 session）。
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| RuntimeError(format!("take_writer 失败: {e}")))?;

        let session = CliSession {
            master: pair.master,
            writer: Some(writer),
            child,
            killer,
            killed: false,
            reader_task: None,
        };

        self.sessions
            .lock()
            .await
            .insert(spec.agent_id.clone(), session);

        Ok(AgentHandle {
            agent_id: spec.agent_id,
        })
    }

    async fn send(
        &self,
        handle: &AgentHandle,
        prompt: String,
    ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
        let (tx, rx) = mpsc::unbounded_channel::<AgentEvent>();

        let mut sessions = self.sessions.lock().await;
        let Some(session) = sessions.get_mut(&handle.agent_id) else {
            return Err(RuntimeError(format!(
                "agent_id={} 的会话不存在",
                handle.agent_id
            )));
        };

        // 串行约束：若上一轮 reader 还在跑，等它结束（CLI 一次处理一条 prompt）。
        if let Some(prev) = session.reader_task.take() {
            let _ = prev.await; // 容忍 panic：prev 已退出。
        }

        // 1. 写 prompt + 换行到 PTY stdin（"\n" 即可，PTY 会自动转 CR/LF）。
        let writer = session
            .writer
            .as_mut()
            .ok_or_else(|| RuntimeError("writer 已被消费（send 不能重入）".to_string()))?;
        writer
            .write_all(format!("{prompt}\n").as_bytes())
            .map_err(|e| RuntimeError(format!("写 PTY 失败: {e}")))?;
        writer
            .flush()
            .map_err(|e| RuntimeError(format!("flush PTY 失败: {e}")))?;

        // 2. clone 出一份 reader 给读任务。master 在 session 里保留，下次 send 还能再 clone。
        let reader = session
            .master
            .try_clone_reader()
            .map_err(|e| RuntimeError(format!("try_clone_reader 失败: {e}")))?;

        // 3. spawn 阻塞读任务：按行读、按行映射成 TextDelta；EOF 后 spawn wait 任务发 Done。
        let sessions_clone = Arc::clone(&self.sessions);
        let agent_id = handle.agent_id.clone();
        let task = tokio::task::spawn_blocking(move || {
            // BufReader 按行切分：PTY 输出未必按 `\n` 整齐到达，但 BufRead 会缓冲。
            let mut lines = BufReader::new(reader).lines();
            while let Some(Ok(line)) = lines.next() {
                // 归一化：去尾 `\r`（PTY CRLF），trim_end 后作为 TextDelta。
                let normalized = line.trim_end_matches('\r');
                if tx.send(AgentEvent::TextDelta(normalized.to_string())).is_err() {
                    // 调用方丢弃 receiver：停止转发。
                    return;
                }
            }
            // reader EOF：子进程关闭了 stdout，意味着即将退出。
            // spawn 一个 async 任务调 wait 拿 exit status（避免阻塞 spawn_blocking 线程）。
            tokio::spawn(async move {
                let mut sessions = sessions_clone.lock().await;
                let Some(session) = sessions.get_mut(&agent_id) else {
                    return; // session 已被 stop 移除。
                };
                // child.wait() 阻塞，但通常很快（子进程已退出）。
                let success = match session.child.wait() {
                    Ok(status) => status.success(),
                    // wait 失败按失败处理（与 std 约定一致）。
                    Err(_) => false,
                };
                let _ = tx.send(AgentEvent::Done {
                    success,
                    // files_modified：CLI 输出不携带文件变更信息，恒为空。
                    // 文件级变更检测必须由 Coordinator 在 done 后做工作区 diff。
                    files_modified: vec![],
                });
                session.killed = true; // 标记已退出，避免后续 abort 再 kill。
            });
        });

        // 记录 reader task，下次 send 会先 await 它。
        let session = sessions.get_mut(&handle.agent_id).expect("刚 insert/取过");
        session.reader_task = Some(task);

        Ok(rx)
    }

    async fn abort(&self, handle: &AgentHandle) -> Result<(), RuntimeError> {
        let mut sessions = self.sessions.lock().await;
        let Some(session) = sessions.get_mut(&handle.agent_id) else {
            return Ok(()); // 会话不存在视为已 abort，幂等。
        };
        if session.killed {
            return Ok(());
        }
        // kill 整个进程组：portable-pty 的 ChildKiller::kill 在 Unix 上先 SIGHUP
        // （PTY slave 子进程作为 session leader 会把信号扩散给同 session 进程），
        // 不死再走 std::process::Child::kill（SIGKILL）兜底。
        session
            .killer
            .kill()
            .map_err(|e| RuntimeError(e.to_string()))?;
        session.killed = true;
        Ok(())
    }

    async fn liveness(&self, handle: &AgentHandle) -> Liveness {
        let mut sessions = self.sessions.lock().await;
        let Some(session) = sessions.get_mut(&handle.agent_id) else {
            return Liveness::Dead;
        };
        // try_wait 不阻塞：返回 Some → 已退出 → Dead；None → 仍活 → Alive。
        match session.child.try_wait() {
            Ok(Some(_)) => Liveness::Dead,
            Ok(None) => Liveness::Alive,
            Err(_) => Liveness::Dead, // wait 出错通常意味着僵尸进程已不可达。
        }
    }

    async fn stop(&self, handle: &AgentHandle) -> Result<(), RuntimeError> {
        // 先 abort（发 kill 信号），再 wait 收尸，最后移除 session。
        let mut sessions = self.sessions.lock().await;
        let Some(mut session) = sessions.remove(&handle.agent_id) else {
            return Ok(()); // 幂等。
        };
        if !session.killed {
            let _ = session.killer.kill(); // stop 容忍 kill 失败（进程可能已退出）。
            session.killed = true;
        }
        // 阻塞 wait：相对 abort 多一步回收资源，避免僵尸。
        let _ = session.child.wait();
        // 显式 drop：释放 writer / master，PTY 句柄关掉。
        drop(session);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    /// 辅助：构造一个 `RuntimeStartSpec`，cwd = 给定路径。
    fn spec(agent_id: &str, cwd: PathBuf) -> RuntimeStartSpec {
        RuntimeStartSpec {
            agent_id: agent_id.to_string(),
            cwd,
            model: "claude-test".to_string(),
            provider: "claude".to_string(),
        }
    }

    /// 辅助：从 receiver 收集事件，直到收到 `Done`（或 channel 关闭）。
    ///
    /// 不在 assertion 里用 `time::sleep`（避免 nondeterminism）：PTY 子进程的输出
    /// 在 send 后已发出，调度循环总能跑到。用 `yield_now` 让出调度，最多等 2000
    /// 次以兜底（远大于任何实际所需）。
    async fn collect_events(mut rx: mpsc::UnboundedReceiver<AgentEvent>) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        for _ in 0..2000 {
            match rx.try_recv() {
                Ok(ev) => {
                    let is_done = matches!(ev, AgentEvent::Done { .. });
                    out.push(ev);
                    if is_done {
                        // Done 是终止事件；channel 可能还有缓冲事件，drain 之。
                        while let Ok(extra) = rx.try_recv() {
                            out.push(extra);
                        }
                        return out;
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    tokio::task::yield_now().await;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => return out,
            }
        }
        out
    }

    // ===== Step 1: happy path —— echo hi; exit 0 → TextDelta("hi") + Done{success:true} =====

    #[tokio::test(flavor = "current_thread")]
    async fn happy_path_echo_then_done_success() {
        let tmp = tempfile::tempdir().unwrap();
        let rt = CliRuntime::new(vec![
            "bash".into(),
            "-c".into(),
            "echo hi; exit 0".into(),
        ]);
        let handle = rt
            .start(spec("a1", tmp.path().to_path_buf()))
            .await
            .expect("start ok");
        let rx = rt.send(&handle, "".into()).await.expect("send ok");
        let events = collect_events(rx).await;

        // 期望：至少一个 TextDelta 含 "hi"；至少一个 Done{success:true}。
        let has_hi = events.iter().any(|ev| {
            matches!(ev, AgentEvent::TextDelta(t) if t.contains("hi"))
        });
        let has_done_ok = events
            .iter()
            .any(|ev| matches!(ev, AgentEvent::Done { success: true, .. }));
        assert!(has_hi, "应有 TextDelta 含 'hi'，实际: {events:?}");
        assert!(has_done_ok, "应有 Done{{success:true}}，实际: {events:?}");

        rt.stop(&handle).await.expect("stop ok");
    }

    // ===== Step 1b: 非零退出 → Done{success:false} =====

    #[tokio::test(flavor = "current_thread")]
    async fn non_zero_exit_yields_done_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let rt = CliRuntime::new(vec![
            "bash".into(),
            "-c".into(),
            "echo oops 1>&2; exit 1".into(),
        ]);
        let handle = rt
            .start(spec("a2", tmp.path().to_path_buf()))
            .await
            .expect("start ok");
        let rx = rt.send(&handle, "".into()).await.expect("send ok");
        let events = collect_events(rx).await;

        let has_done_fail = events
            .iter()
            .any(|ev| matches!(ev, AgentEvent::Done { success: false, .. }));
        assert!(has_done_fail, "应有 Done{{success:false}}，实际: {events:?}");

        rt.stop(&handle).await.expect("stop ok");
    }

    // ===== Step 3: 多行输出 → 两个 TextDelta + Done{success:true} =====

    #[tokio::test(flavor = "current_thread")]
    async fn multi_line_output_yields_two_text_deltas() {
        let tmp = tempfile::tempdir().unwrap();
        let rt = CliRuntime::new(vec![
            "bash".into(),
            "-c".into(),
            "echo a; echo b; exit 0".into(),
        ]);
        let handle = rt
            .start(spec("a3", tmp.path().to_path_buf()))
            .await
            .expect("start ok");
        let rx = rt.send(&handle, "".into()).await.expect("send ok");
        let events = collect_events(rx).await;

        // 期望：至少两个 TextDelta（含 "a" 和 "b"），加一个 Done{success:true}。
        let text_deltas: Vec<String> = events
            .iter()
            .filter_map(|ev| match ev {
                AgentEvent::TextDelta(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        let has_a = text_deltas.iter().any(|t| t.contains('a'));
        let has_b = text_deltas.iter().any(|t| t.contains('b'));
        let has_done_ok = events
            .iter()
            .any(|ev| matches!(ev, AgentEvent::Done { success: true, .. }));
        assert!(
            has_a,
            "应有 TextDelta 含 'a'，实际 text_deltas: {text_deltas:?}"
        );
        assert!(
            has_b,
            "应有 TextDelta 含 'b'，实际 text_deltas: {text_deltas:?}"
        );
        assert!(has_done_ok, "应有 Done{{success:true}}，实际: {events:?}");

        rt.stop(&handle).await.expect("stop ok");
    }

    // ===== Step 4: abort 长任务后 liveness == Dead =====

    #[tokio::test(flavor = "current_thread")]
    async fn abort_long_running_makes_liveness_dead() {
        let tmp = tempfile::tempdir().unwrap();
        let rt = CliRuntime::new(vec![
            "bash".into(),
            "-c".into(),
            // sleep 30 是子进程自己的指令，不依赖 stdin 输入。
            "sleep 30".into(),
        ]);
        let handle = rt
            .start(spec("a4", tmp.path().to_path_buf()))
            .await
            .expect("start ok");
        // 让 reader 启动（spawn_blocking 进入阻塞读）。send 一条空 prompt 即可。
        let _rx = rt.send(&handle, "".into()).await.expect("send ok");

        // 先确认进程是 alive（spawn 后立即 check）。
        let liveness_before = rt.liveness(&handle).await;
        assert_eq!(liveness_before, Liveness::Alive, "spawn 后应 Alive");

        // abort：发 SIGHUP/SIGKILL。
        rt.abort(&handle).await.expect("abort ok");

        // 等子进程真的退出（kill 是异步的：SIGHUP 发出后内核调度子进程退出）。
        // 用 bounded 轮询 + try_wait 探测，最多等 ~2s（远超 kill 传播延迟）。
        // 这是必要的 exception —— kill 是异步内核信号，无法纯 yield 等待。
        let mut dead = false;
        for _ in 0..400 {
            if rt.liveness(&handle).await == Liveness::Dead {
                dead = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(dead, "abort 后 liveness 应为 Dead");

        rt.stop(&handle).await.expect("stop ok");
    }

    // ===== 容错：未知 agent_id 的 abort/stop/liveness 幂等返回 =====

    #[tokio::test(flavor = "current_thread")]
    async fn unknown_agent_id_is_idempotent() {
        let _tmp = tempfile::tempdir().unwrap();
        let rt = CliRuntime::new(vec!["bash".into(), "-c".into(), "true".into()]);

        let ghost = AgentHandle {
            agent_id: "nope".to_string(),
        };
        rt.abort(&ghost).await.expect("abort 幂等");
        rt.stop(&ghost).await.expect("stop 幂等");
        assert_eq!(rt.liveness(&ghost).await, Liveness::Dead);

        // capabilities 不依赖 session，应直接返回 degraded tier。
        let caps = rt.capabilities();
        assert!(!caps.structured_events);
        assert!(!caps.supports_resume);
        assert!(!caps.supports_permission_prompt);
    }

    // ===== capabilities 标记 degraded tier =====

    #[test]
    fn capabilities_is_degraded_tier() {
        let rt = CliRuntime::new(vec!["bash".into()]);
        let caps = rt.capabilities();
        assert!(
            !caps.structured_events,
            "CliRuntime 必须 structured_events=false"
        );
        assert!(!caps.supports_resume);
        assert!(!caps.supports_permission_prompt);
    }
}
