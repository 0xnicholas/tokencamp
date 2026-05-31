use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub providers: HashMap<String, ProviderEntry>,
    pub model_list: Vec<ModelEntry>,
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

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderEntry {
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelEntry {
    pub model_name: String,
    pub provider: String,
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
