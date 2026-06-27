//! 按 agent_id 索引的 daemon 池。
//! 每个 agent 一个独立 DaemonClient（独立进程/cwd/session）。
//! 取代原先 ChatManager 的单例 OnceCell。

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::agent_id::AgentId;
use super::manager::ManagerDaemonClient;

pub struct AgentPool {
    clients: Mutex<HashMap<AgentId, Arc<dyn ManagerDaemonClient>>>,
}

impl AgentPool {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// 取缓存；不存在则用 `init` 创建并缓存。init 失败不写入。
    /// 双检：并发下若他人已抢先写入同一 id，复用既有 client，丢弃本次。
    pub async fn get_or_init<F, Fut>(
        &self,
        id: &AgentId,
        init: F,
    ) -> Result<Arc<dyn ManagerDaemonClient>, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<dyn ManagerDaemonClient>, String>>,
    {
        if let Some(c) = self.clients.lock().await.get(id) {
            return Ok(c.clone());
        }
        let client = init().await?;
        let mut guard = self.clients.lock().await;
        // 双检：并发下若他人已写入，复用之，丢弃本次。
        if let Some(existing) = guard.get(id) {
            return Ok(existing.clone());
        }
        guard.insert(id.clone(), client.clone());
        Ok(client)
    }

    pub async fn get(&self, id: &AgentId) -> Option<Arc<dyn ManagerDaemonClient>> {
        self.clients.lock().await.get(id).cloned()
    }

    pub async fn ids(&self) -> Vec<AgentId> {
        self.clients.lock().await.keys().cloned().collect()
    }

    pub async fn remove(&self, id: &AgentId) -> Option<Arc<dyn ManagerDaemonClient>> {
        self.clients.lock().await.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::AgentPool;
    use crate::chat::manager::ManagerDaemonClient;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // 复用 manager 暴露的最小 fake daemon client。
    fn fake() -> Arc<dyn ManagerDaemonClient> {
        crate::chat::manager::test_support::fake_client()
    }

    #[tokio::test]
    async fn get_or_init_caches_per_id() {
        let pool = AgentPool::new();
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let calls = calls.clone();
            pool.get_or_init(&"a".to_string(), || async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(fake())
            })
            .await
            .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "同一 id 只初始化一次");
        assert_eq!(pool.ids().await, vec!["a".to_string()]);
    }

    #[tokio::test]
    async fn remove_drops_entry() {
        let pool = AgentPool::new();
        pool.get_or_init(&"a".to_string(), || async { Ok(fake()) })
            .await
            .unwrap();
        assert!(pool.remove(&"a".to_string()).await.is_some());
        assert!(pool.get(&"a".to_string()).await.is_none());
    }
}
