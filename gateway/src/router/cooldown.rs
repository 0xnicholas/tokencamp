// gateway/src/router/cooldown.rs
use redis::aio::MultiplexedConnection;
use std::collections::HashMap;
use std::sync::Mutex;

pub enum CooldownBackend {
    Redis(MultiplexedConnection),
    InMemory(Mutex<CooldownState>),
}

pub struct CooldownState {
    cooldowns: HashMap<String, std::time::Instant>,
    failures: HashMap<String, u32>,
}

pub struct CooldownManager {
    backend: CooldownBackend,
}

impl CooldownManager {
    pub fn new_redis(conn: MultiplexedConnection) -> Self {
        Self {
            backend: CooldownBackend::Redis(conn),
        }
    }

    pub fn new_in_memory() -> Self {
        Self {
            backend: CooldownBackend::InMemory(Mutex::new(CooldownState {
                cooldowns: HashMap::new(),
                failures: HashMap::new(),
            })),
        }
    }

    pub async fn mark(&self, deployment_id: &str, ttl_secs: u64) -> Result<(), String> {
        match &self.backend {
            CooldownBackend::Redis(conn) => {
                let mut conn = conn.clone();
                redis::cmd("SETEX")
                    .arg(format!("deployment:{}:cooldown", deployment_id))
                    .arg(ttl_secs)
                    .arg("1")
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| e.to_string())
            }
            CooldownBackend::InMemory(state) => {
                state.lock().unwrap().cooldowns.insert(
                    deployment_id.to_string(),
                    std::time::Instant::now() + std::time::Duration::from_secs(ttl_secs),
                );
                Ok(())
            }
        }
    }

    pub async fn is_cooling_down(&self, deployment_id: &str) -> bool {
        match &self.backend {
            CooldownBackend::Redis(conn) => {
                let mut conn = conn.clone();
                redis::cmd("EXISTS")
                    .arg(format!("deployment:{}:cooldown", deployment_id))
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(0i32)
                    > 0
            }
            CooldownBackend::InMemory(state) => {
                if let Some(expiry) = state.lock().unwrap().cooldowns.get(deployment_id) {
                    *expiry > std::time::Instant::now()
                } else {
                    false
                }
            }
        }
    }

    /// 记录失败，返回当前连续失败次数
    pub async fn record_failure(&self, deployment_id: &str) -> u32 {
        match &self.backend {
            CooldownBackend::Redis(conn) => {
                let mut conn = conn.clone();
                let key = format!("deployment:{}:failures", deployment_id);
                let count: u32 = redis::cmd("INCR")
                    .arg(&key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(1);
                // 设置 TTL 防止泄漏
                let _: () = redis::cmd("EXPIRE")
                    .arg(&key)
                    .arg(120) // 2 min
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(());
                count
            }
            CooldownBackend::InMemory(state) => {
                let mut state = state.lock().unwrap();
                let count = state.failures.entry(deployment_id.to_string()).or_insert(0);
                *count += 1;
                *count
            }
        }
    }

    /// 一次成功调用后重置失败计数器
    pub async fn record_success(&self, deployment_id: &str) {
        match &self.backend {
            CooldownBackend::Redis(conn) => {
                let mut conn = conn.clone();
                let _: () = redis::cmd("DEL")
                    .arg(format!("deployment:{}:failures", deployment_id))
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(());
            }
            CooldownBackend::InMemory(state) => {
                state.lock().unwrap().failures.remove(deployment_id);
            }
        }
    }
}
