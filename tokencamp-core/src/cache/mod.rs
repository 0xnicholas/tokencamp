use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lru::LruCache;

use crate::provider::ProviderError;

/// 抽象缓存层，支持 Redis 和 In-memory fallback
#[async_trait::async_trait]
pub trait CacheLayer: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn set(&self, key: &str, value: &str, ttl: Duration);
    async fn del(&self, key: &str);
    async fn incr(&self, key: &str) -> Result<u32, String>;
    async fn expire(&self, key: &str, ttl: u64);
    async fn rpush(&self, key: &str, value: &str);
}

enum DualCacheBackend {
    Redis(redis::aio::MultiplexedConnection),
    InMemory {
        store: Mutex<HashMap<String, String>>,
        counters: Mutex<HashMap<String, u32>>,
        lists: Mutex<HashMap<String, Vec<String>>>,
    },
}

pub struct DualCache {
    in_memory: Mutex<LruCache<String, (String, Instant)>>,
    backend: DualCacheBackend,
}

impl DualCache {
    pub fn new_in_memory(capacity: usize) -> Self {
        Self {
            in_memory: Mutex::new(LruCache::new(std::num::NonZeroUsize::new(capacity).unwrap())),
            backend: DualCacheBackend::InMemory {
                store: Mutex::new(HashMap::new()),
                counters: Mutex::new(HashMap::new()),
                lists: Mutex::new(HashMap::new()),
            },
        }
    }

    pub fn new_redis(capacity: usize, conn: redis::aio::MultiplexedConnection) -> Self {
        Self {
            in_memory: Mutex::new(LruCache::new(std::num::NonZeroUsize::new(capacity).unwrap())),
            backend: DualCacheBackend::Redis(conn),
        }
    }
}

#[async_trait::async_trait]
impl CacheLayer for DualCache {
    async fn get(&self, key: &str) -> Option<String> {
        // 1. in-memory
        {
            let mut cache = self.in_memory.lock().unwrap();
            if let Some((val, expiry)) = cache.get(key) {
                if *expiry > Instant::now() {
                    return Some(val.clone());
                }
                cache.pop(key);
            }
        }
        // 2. Redis / InMemory
        match &self.backend {
            DualCacheBackend::Redis(conn) => {
                let mut conn = conn.clone();
                let result: Option<String> = redis::cmd("GET")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .ok()?;
                if let Some(ref val) = result {
                    let mut cache = self.in_memory.lock().unwrap();
                    cache.put(key.to_string(), (val.clone(), Instant::now() + Duration::from_secs(60)));
                }
                result
            }
            DualCacheBackend::InMemory { store, counters, .. } => {
                // 优先查 store，其次查 counters（incr 写入的目标）
                if let Some(val) = store.lock().unwrap().get(key) {
                    return Some(val.clone());
                }
                counters.lock().unwrap().get(key).map(|v| v.to_string())
            }
        }
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) {
        // in-memory
        {
            let mut cache = self.in_memory.lock().unwrap();
            cache.put(key.to_string(), (value.to_string(), Instant::now() + ttl));
        }
        // backend
        match &self.backend {
            DualCacheBackend::Redis(conn) => {
                let mut conn = conn.clone();
                let _: () = redis::cmd("SETEX")
                    .arg(key)
                    .arg(ttl.as_secs())
                    .arg(value)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(());
            }
            DualCacheBackend::InMemory { store, .. } => {
                store.lock().unwrap().insert(key.to_string(), value.to_string());
            }
        }
    }

    async fn del(&self, key: &str) {
        {
            let mut cache = self.in_memory.lock().unwrap();
            cache.pop(key);
        }
        match &self.backend {
            DualCacheBackend::Redis(conn) => {
                let mut conn = conn.clone();
                let _: () = redis::cmd("DEL").arg(key).query_async(&mut conn).await.unwrap_or(());
            }
            DualCacheBackend::InMemory { store, .. } => {
                store.lock().unwrap().remove(key);
            }
        }
    }

    async fn incr(&self, key: &str) -> Result<u32, String> {
        match &self.backend {
            DualCacheBackend::Redis(conn) => {
                let mut conn = conn.clone();
                redis::cmd("INCR")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| e.to_string())
            }
            DualCacheBackend::InMemory { counters, .. } => {
                let mut counters = counters.lock().unwrap();
                let count = counters.entry(key.to_string()).or_insert(0);
                *count += 1;
                Ok(*count)
            }
        }
    }

    async fn expire(&self, key: &str, ttl: u64) {
        match &self.backend {
            DualCacheBackend::Redis(conn) => {
                let mut conn = conn.clone();
                let _: () = redis::cmd("EXPIRE")
                    .arg(key)
                    .arg(ttl)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(());
            }
            DualCacheBackend::InMemory { .. } => {
                // in-memory: no TTL support for counters, they reset via new window hash
            }
        }
    }

    async fn rpush(&self, key: &str, value: &str) {
        match &self.backend {
            DualCacheBackend::Redis(conn) => {
                let mut conn = conn.clone();
                let _: () = redis::cmd("RPUSH")
                    .arg(key)
                    .arg(value)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(());
            }
            DualCacheBackend::InMemory { lists, .. } => {
                lists.lock().unwrap()
                    .entry(key.to_string())
                    .or_default()
                    .push(value.to_string());
            }
        }
    }
}
