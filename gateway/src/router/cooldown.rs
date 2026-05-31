// gateway/src/router/cooldown.rs
use redis::aio::MultiplexedConnection;
use std::collections::HashMap;
use std::sync::Mutex;

pub enum CooldownBackend {
    Redis(MultiplexedConnection),
    InMemory(Mutex<HashMap<String, std::time::Instant>>),
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
            backend: CooldownBackend::InMemory(Mutex::new(HashMap::new())),
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
            CooldownBackend::InMemory(map) => {
                map.lock().unwrap().insert(
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
            CooldownBackend::InMemory(map) => {
                if let Some(expiry) = map.lock().unwrap().get(deployment_id) {
                    *expiry > std::time::Instant::now()
                } else {
                    false
                }
            }
        }
    }
}
