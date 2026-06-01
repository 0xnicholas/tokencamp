use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub general_settings: GeneralSettings,
    pub providers: HashMap<String, ProviderEntry>,
    pub model_list: Vec<ModelEntry>,
    #[serde(default)]
    pub router_settings: RouterSettings,
    #[serde(default)]
    pub redis: RedisConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    pub database_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub api_keys: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GeneralSettings {
    pub master_key: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderEntry {
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelEntry {
    pub model_name: String,
    pub provider: String,
    #[serde(default)]
    pub litellm_params: Option<LitellmParams>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LitellmParams {
    pub model: String,
    #[serde(default)]
    pub model_info: Option<ModelInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelInfo {
    pub prompt_price: f64,
    pub completion_price: f64,
    #[serde(default)]
    pub tpm: Option<u32>,
    #[serde(default)]
    pub rpm: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouterSettings {
    #[serde(default = "default_num_retries")]
    pub num_retries: u32,
    #[serde(default = "default_retry_after")]
    pub retry_after: f64,
    #[serde(default = "default_allowed_fails")]
    pub allowed_fails: u32,
    #[serde(default = "default_cooldown_time")]
    pub cooldown_time: u64,
    #[serde(default)]
    pub fallbacks: HashMap<String, Vec<String>>,
    #[serde(default = "default_routing_strategy")]
    pub routing_strategy: String,
    #[serde(default = "default_latency_window")]
    pub latency_window_size: usize,
}

fn default_num_retries() -> u32 { 3 }
fn default_retry_after() -> f64 { 0.5 }
fn default_allowed_fails() -> u32 { 3 }
fn default_cooldown_time() -> u64 { 30 }
fn default_routing_strategy() -> String { "simple_shuffle".into() }
fn default_latency_window() -> usize { 10 }

impl Default for RouterSettings {
    fn default() -> Self {
        Self {
            num_retries: default_num_retries(),
            retry_after: default_retry_after(),
            allowed_fails: default_allowed_fails(),
            cooldown_time: default_cooldown_time(),
            fallbacks: HashMap::new(),
            routing_strategy: default_routing_strategy(),
            latency_window_size: default_latency_window(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct RedisConfig {
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub enabled: Vec<String>,
}

/// 加载 YAML 配置，解析 ${ENV_VAR} 占位符
pub fn load(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(path)?;
    let resolved = resolve_env_vars(&raw);
    let config: Config = serde_yaml::from_str(&resolved)?;
    Ok(config)
}

fn resolve_env_vars(raw: &str) -> String {
    let mut result = raw.to_string();
    for cap in raw.match_indices("${") {
        let start = cap.0;
        if let Some(end) = raw[start..].find('}') {
            let var_name = &raw[start + 2..start + end];
            if let Ok(val) = std::env::var(var_name) {
                result = result.replace(&format!("${{{}}}", var_name), &val);
            }
        }
    }
    result
}
