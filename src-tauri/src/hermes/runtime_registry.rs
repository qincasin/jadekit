//! 介质注册表：一次 run 内按 task.assignment.runtime 选具体介质。
//!
//! Phase 2 的 Coordinator/Supervisor 持单个 `Arc<dyn AgentRuntime>`；Phase 3b 升级为
//! `RuntimeKind → Arc<dyn AgentRuntime>` 注册表，让一次 run 内 SDK × CLI 异构混跑
//! （按 task.assignment.runtime 选介质）。本模块只实现注册表原语；接入由 Task 7/8/9 完成。

use std::collections::HashMap;
use std::sync::Arc;

use crate::hermes::runtime::AgentRuntime;
use crate::hermes::types::RuntimeKind;

/// 介质注册表。线程安全（构造期填入后只读；内部全是 `Arc`，`Clone` 廉价）。
#[derive(Clone)]
pub struct RuntimeRegistry {
    runtimes: HashMap<RuntimeKind, Arc<dyn AgentRuntime>>,
}

impl RuntimeRegistry {
    /// 空注册表。
    pub fn new() -> Self {
        Self { runtimes: HashMap::new() }
    }

    /// builder：登记一个介质（覆盖同 kind 的旧值）。
    pub fn with(mut self, kind: RuntimeKind, rt: Arc<dyn AgentRuntime>) -> Self {
        self.runtimes.insert(kind, rt);
        self
    }

    /// 取某介质；缺失返回 Err（调用方决定 fail_dispatch）。
    pub fn get(&self, kind: RuntimeKind) -> Result<Arc<dyn AgentRuntime>, String> {
        self.runtimes
            .get(&kind)
            .cloned()
            .ok_or_else(|| format!("RuntimeRegistry: 未登记 runtime kind {:?}", kind))
    }

    /// 单介质便捷构造（兼容 Phase 2：同一 rt 登记到所有 kind，
    /// 这样无论 task.assignment.runtime 是什么都能拿到它 → 行为与 Phase 2 逐字一致）。
    pub fn single(rt: Arc<dyn AgentRuntime>) -> Self {
        Self {
            runtimes: [(RuntimeKind::Sdk, rt.clone()), (RuntimeKind::Cli, rt)]
                .into_iter()
                .collect(),
        }
    }

    /// 已登记的所有介质种类（Task 8 supervisor liveness 探测按 agent 介质查时用）。
    pub fn kinds(&self) -> Vec<RuntimeKind> {
        self.runtimes.keys().copied().collect()
    }
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hermes::{
        AgentEvent, AgentHandle, Liveness, RuntimeCapabilities, RuntimeError, RuntimeStartSpec,
    };
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    // ── 测试专用 Mock 介质：两个不同 unit struct，便于用类型区分 ──────────────
    //
    // 单元结构体 `SdkMock` / `CliMock` 各自实现 `AgentRuntime`（全 no-op），
    // 注册到 `RuntimeRegistry` 后用 `Arc::ptr_eq` 验证 `get(...)` 取回的是同一指针。
    // 风格镜像 `coordinator.rs::tests::MockRuntime` / `supervisor.rs::tests::MockRuntime`。

    /// 模拟 SDK 介质。
    struct SdkMock;

    #[async_trait]
    impl AgentRuntime for SdkMock {
        fn capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities {
                structured_events: true,
                supports_resume: false,
                supports_permission_prompt: false,
            }
        }
        async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError> {
            Ok(AgentHandle { agent_id: spec.agent_id })
        }
        async fn send(
            &self,
            _handle: &AgentHandle,
            _prompt: String,
        ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
            let (_tx, rx) = mpsc::unbounded_channel();
            Ok(rx)
        }
        async fn abort(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }
        async fn liveness(&self, _handle: &AgentHandle) -> Liveness {
            Liveness::Alive
        }
        async fn stop(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    /// 模拟 CLI 介质（与 SdkMock 行为一致，仅类型不同）。
    struct CliMock;

    #[async_trait]
    impl AgentRuntime for CliMock {
        fn capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities {
                structured_events: false,
                supports_resume: false,
                supports_permission_prompt: false,
            }
        }
        async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError> {
            Ok(AgentHandle { agent_id: spec.agent_id })
        }
        async fn send(
            &self,
            _handle: &AgentHandle,
            _prompt: String,
        ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
            let (_tx, rx) = mpsc::unbounded_channel();
            Ok(rx)
        }
        async fn abort(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }
        async fn liveness(&self, _handle: &AgentHandle) -> Liveness {
            Liveness::Alive
        }
        async fn stop(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    // 用例 1：`with(Sdk, a).with(Cli, b)` 后 `get(Sdk)`/`get(Cli)` 各自命中，
    //         `get(缺失 kind)` Err（当前枚举只有 Sdk/Cli 两值，这里空注册表验证）。
    #[test]
    fn with_chains_distinct_kinds_and_get_returns_right_pointer() {
        let sdk: Arc<dyn AgentRuntime> = Arc::new(SdkMock);
        let cli: Arc<dyn AgentRuntime> = Arc::new(CliMock);

        let reg = RuntimeRegistry::new()
            .with(RuntimeKind::Sdk, sdk.clone())
            .with(RuntimeKind::Cli, cli.clone());

        // get(Sdk) == Ok(sdk)
        let got_sdk = reg.get(RuntimeKind::Sdk).expect("Sdk must be registered");
        assert!(
            Arc::ptr_eq(&got_sdk, &sdk),
            "get(Sdk) 必须返回登记的 sdk 指针"
        );
        // 不能误中 cli
        assert!(
            !Arc::ptr_eq(&got_sdk, &cli),
            "get(Sdk) 不能返回 cli 指针"
        );

        // get(Cli) == Ok(cli)
        let got_cli = reg.get(RuntimeKind::Cli).expect("Cli must be registered");
        assert!(
            Arc::ptr_eq(&got_cli, &cli),
            "get(Cli) 必须返回登记的 cli 指针"
        );
        assert!(
            !Arc::ptr_eq(&got_cli, &sdk),
            "get(Cli) 不能返回 sdk 指针"
        );
    }

    // 用例 1b：`get` 对未登记的 kind 返回 Err。
    #[test]
    fn get_unregistered_kind_is_err() {
        // 空注册表 → 任意 kind 都未登记。
        let reg = RuntimeRegistry::new();
        assert!(reg.get(RuntimeKind::Sdk).is_err(), "空注册表 get(Sdk) 必须 Err");
        assert!(reg.get(RuntimeKind::Cli).is_err(), "空注册表 get(Cli) 必须 Err");

        // 只登记 Sdk → get(Cli) 仍 Err。
        let reg = RuntimeRegistry::new().with(RuntimeKind::Sdk, Arc::new(SdkMock));
        assert!(reg.get(RuntimeKind::Cli).is_err(), "未登记 Cli 时 get(Cli) 必须 Err");
    }

    // 用例 2：`single(rt)` 后 `get(Sdk)` 与 `get(Cli)` 都 ptr_eq 到 rt（兼容 Phase 2）。
    #[test]
    fn single_registers_same_pointer_for_all_kinds() {
        let rt: Arc<dyn AgentRuntime> = Arc::new(SdkMock);
        let reg = RuntimeRegistry::single(rt.clone());

        let got_sdk = reg.get(RuntimeKind::Sdk).expect("single: get(Sdk) 必须 Ok");
        let got_cli = reg.get(RuntimeKind::Cli).expect("single: get(Cli) 必须 Ok");

        assert!(
            Arc::ptr_eq(&got_sdk, &rt),
            "single: get(Sdk) 必须 ptr_eq 到原 rt"
        );
        assert!(
            Arc::ptr_eq(&got_cli, &rt),
            "single: get(Cli) 必须 ptr_eq 到原 rt"
        );
        // 两个 kind 取回的也是同一指针。
        assert!(
            Arc::ptr_eq(&got_sdk, &got_cli),
            "single: get(Sdk) 与 get(Cli) 必须 ptr_eq"
        );
    }

    // 用例 3：`with` 覆盖同 kind 的旧值（后登记覆盖先登记）。
    #[test]
    fn with_overrides_same_kind() {
        let first: Arc<dyn AgentRuntime> = Arc::new(SdkMock);
        let second: Arc<dyn AgentRuntime> = Arc::new(CliMock);

        let reg = RuntimeRegistry::new()
            .with(RuntimeKind::Sdk, first.clone())
            .with(RuntimeKind::Sdk, second.clone());

        let got = reg.get(RuntimeKind::Sdk).expect("Sdk 必须已登记");
        assert!(
            Arc::ptr_eq(&got, &second),
            "with(Sdk, _) 第二次必须覆盖第一次"
        );
        assert!(
            !Arc::ptr_eq(&got, &first),
            "覆盖后旧指针不应再被取回"
        );
    }

    // 用例 4：`kinds()` 返回所有已登记 kind（顺序无关，逐个 contains 校验）。
    #[test]
    fn kinds_returns_all_registered() {
        // 空注册表 → kinds() 为空。
        let reg = RuntimeRegistry::new();
        assert!(reg.kinds().is_empty(), "空注册表 kinds() 必须为空");

        // 单 kind。
        let reg = RuntimeRegistry::new().with(RuntimeKind::Sdk, Arc::new(SdkMock));
        let kinds = reg.kinds();
        assert_eq!(kinds.len(), 1);
        assert!(kinds.contains(&RuntimeKind::Sdk));

        // 两 kind。
        let reg = RuntimeRegistry::new()
            .with(RuntimeKind::Sdk, Arc::new(SdkMock))
            .with(RuntimeKind::Cli, Arc::new(CliMock));
        let kinds = reg.kinds();
        assert_eq!(kinds.len(), 2, "两种介质都登记后 kinds() 应有 2 项");
        assert!(kinds.contains(&RuntimeKind::Sdk), "kinds() 必须含 Sdk");
        assert!(kinds.contains(&RuntimeKind::Cli), "kinds() 必须含 Cli");

        // single() → 两种 kind 都登记。
        let reg = RuntimeRegistry::single(Arc::new(SdkMock));
        let kinds = reg.kinds();
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains(&RuntimeKind::Sdk));
        assert!(kinds.contains(&RuntimeKind::Cli));
    }

    // 用例 5：`Default` 等价于 `new()`（空注册表）。
    #[test]
    fn default_is_empty_like_new() {
        let reg = RuntimeRegistry::default();
        assert!(reg.get(RuntimeKind::Sdk).is_err());
        assert!(reg.get(RuntimeKind::Cli).is_err());
        assert!(reg.kinds().is_empty());
    }

    // 用例 6：`Clone` 廉价（内部全是 Arc，clone 后两份 registry 取回同一指针）。
    #[test]
    fn clone_shares_inner_pointers() {
        let sdk: Arc<dyn AgentRuntime> = Arc::new(SdkMock);
        let reg = RuntimeRegistry::new().with(RuntimeKind::Sdk, sdk.clone());
        let reg2 = reg.clone();

        let got = reg.get(RuntimeKind::Sdk).unwrap();
        let got2 = reg2.get(RuntimeKind::Sdk).unwrap();
        assert!(
            Arc::ptr_eq(&got, &got2) && Arc::ptr_eq(&got, &sdk),
            "Clone 后两份 registry 必须共享底层 Arc"
        );
    }
}
