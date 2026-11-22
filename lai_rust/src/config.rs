//! Configuration Management - lai configuration with TOML persistence.
//! Ported from lai.py config handling with full feature parity.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub allow_remote: bool,
    pub temperature: f64,
    pub max_tokens: i32,
    pub stream: bool,
    pub thinking: bool,
    pub tools_enabled: bool,
    pub auto_mode: bool,
    pub yolo: bool,
    pub verbose: u8,
    pub quiet: bool,
    pub lang: String,
    pub token_budget: i64,
    pub rate_limit_per_min: i32,
    pub fallback_url: Option<String>,
    pub services: ServicesConfig,
    #[serde(skip)]
    pub config_path: PathBuf,
    #[serde(skip)]
    pub is_global: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesConfig {
    pub llama_cpp: ServiceConfig,
    pub gateway: ServiceConfig,
    pub ollama: ServiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub port: u16,
    pub health_path: String,
    pub script: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub auto_start: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "qwen3-8b".into(),
            base_url: "http://localhost:8080/v1".into(),
            api_key: None,
            allow_remote: false,
            temperature: 0.7,
            max_tokens: 4096,
            stream: true,
            thinking: true,
            tools_enabled: true,
            auto_mode: false,
            yolo: false,
            verbose: 0,
            quiet: false,
            lang: "es".into(),
            token_budget: 50000,
            rate_limit_per_min: 60,
            fallback_url: Some("http://localhost:11434/v1".into()),
            services: ServicesConfig {
                llama_cpp: ServiceConfig {
                    port: 8080,
                    health_path: "/health".into(),
                    script: Some("/home/kubuntu/llama.cpp-bin/llama-b12567/llama-server".into()),
                    args: vec![],
                    cwd: None,
                    auto_start: false,
                },
                gateway: ServiceConfig {
                    port: 8081,
                    health_path: "/health".into(),
                    script: Some("/home/kubuntu/curso-ia/modelo-local/gateway-go/gateway".into()),
                    args: vec![],
                    cwd: Some("/home/kubuntu/curso-ia/modelo-local/gateway-go".into()),
                    auto_start: false,
                },
                ollama: ServiceConfig {
                    port: 11434,
                    health_path: "/api/tags".into(),
                    script: Some("/usr/bin/ollama".into()),
                    args: vec!["serve".into()],
                    cwd: None,
                    auto_start: false,
                },
            },
            config_path: PathBuf::new(),
            is_global: false,
        }
    }
}

impl Config {
    pub fn load_global() -> Result<Self> {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".lai");
        fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("config.toml");

        let mut config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let mut config: Config = toml::from_str(&content)?;
            config.config_path = config_path;
            config.is_global = true;
            config
        } else {
            let mut config = Config::default();
            config.config_path = config_path;
            config.is_global = true;
            config.save()?;
            config
        };

        config.apply_env_overrides();
        Ok(config)
    }

    pub fn load_local(project_dir: &Path) -> Result<Self> {
        let config_dir = project_dir.join(".lai");
        fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("config.toml");

        let mut config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let mut config: Config = toml::from_str(&content)?;
            config.config_path = config_path;
            config.is_global = false;
            config
        } else {
            let mut config = Config::default();
            config.config_path = config_path;
            config.is_global = false;
            config
        };

        config.apply_env_overrides();
        Ok(config)
    }

    pub fn load_or_default(project_dir: Option<&Path>) -> Result<Self> {
        if let Some(dir) = project_dir {
            if dir.join(".lai").join("config.toml").exists() {
                return Self::load_local(dir);
            }
        }
        Self::load_global()
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(model) = std::env::var("LAI_MODEL") {
            self.model = model;
        }
        if let Ok(url) = std::env::var("LAI_BASE_URL") {
            self.base_url = url;
        }
        if let Ok(key) = std::env::var("LAI_API_KEY") {
            self.api_key = Some(key);
        }
        if let Ok(allow) = std::env::var("LAI_ALLOW_REMOTE") {
            self.allow_remote = allow == "1" || allow.to_lowercase() == "true";
        }
        if let Ok(temp) = std::env::var("LAI_TEMPERATURE") {
            if let Ok(t) = temp.parse() { self.temperature = t; }
        }
        if let Ok(tokens) = std::env::var("LAI_MAX_TOKENS") {
            if let Ok(t) = tokens.parse() { self.max_tokens = t; }
        }
        if let Ok(lang) = std::env::var("LAI_LANG") {
            self.lang = lang;
        }
        if let Ok(model_path) = std::env::var("LAI_LLAMA_MODEL") {
            self.services.llama_cpp.script = Some(model_path);
        }
    }

    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        match key {
            "model" => Some(Value::String(self.model.clone())),
            "base_url" => Some(Value::String(self.base_url.clone())),
            "api_key" => self.api_key.clone().map(Value::String),
            "allow_remote" => Some(Value::Bool(self.allow_remote)),
            "temperature" => Some(serde_json::to_value(self.temperature).unwrap()),
            "max_tokens" => Some(serde_json::to_value(self.max_tokens).unwrap()),
            "stream" => Some(Value::Bool(self.stream)),
            "thinking" => Some(Value::Bool(self.thinking)),
            "tools_enabled" => Some(Value::Bool(self.tools_enabled)),
            "auto_mode" => Some(Value::Bool(self.auto_mode)),
            "yolo" => Some(Value::Bool(self.yolo)),
            "verbose" => Some(serde_json::to_value(self.verbose).unwrap()),
            "quiet" => Some(Value::Bool(self.quiet)),
            "lang" => Some(Value::String(self.lang.clone())),
            "token_budget" => Some(serde_json::to_value(self.token_budget).unwrap()),
            "rate_limit_per_min" => Some(serde_json::to_value(self.rate_limit_per_min).unwrap()),
            "fallback_url" => self.fallback_url.clone().map(Value::String),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: Value) -> Result<()> {
        match key {
            "model" => self.model = value.as_str().unwrap_or("qwen3-8b").into(),
            "base_url" => self.base_url = value.as_str().unwrap_or("http://localhost:8080/v1").into(),
            "api_key" => self.api_key = value.as_str().map(|s| s.to_string()),
            "allow_remote" => self.allow_remote = value.as_bool().unwrap_or(false),
            "temperature" => self.temperature = value.as_f64().unwrap_or(0.7),
            "max_tokens" => self.max_tokens = value.as_i64().unwrap_or(4096) as i32,
            "stream" => self.stream = value.as_bool().unwrap_or(true),
            "thinking" => self.thinking = value.as_bool().unwrap_or(true),
            "tools_enabled" => self.tools_enabled = value.as_bool().unwrap_or(true),
            "auto_mode" => self.auto_mode = value.as_bool().unwrap_or(false),
            "yolo" => self.yolo = value.as_bool().unwrap_or(false),
            "verbose" => self.verbose = value.as_u64().unwrap_or(0) as u8,
            "quiet" => self.quiet = value.as_bool().unwrap_or(false),
            "lang" => self.lang = value.as_str().unwrap_or("es").into(),
            "token_budget" => self.token_budget = value.as_i64().unwrap_or(50000),
            "rate_limit_per_min" => self.rate_limit_per_min = value.as_i64().unwrap_or(60) as i32,
            "fallback_url" => self.fallback_url = value.as_str().map(|s| s.to_string()),
            _ => return Err(anyhow::anyhow!("Unknown config key: {}", key)),
        }
        self.save()
    }

    pub fn list_all(&self) -> Value {
        serde_json::json!({
            "model": self.model,
            "base_url": self.base_url,
            "api_key": self.api_key,
            "allow_remote": self.allow_remote,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "stream": self.stream,
            "thinking": self.thinking,
            "tools_enabled": self.tools_enabled,
            "auto_mode": self.auto_mode,
            "yolo": self.yolo,
            "verbose": self.verbose,
            "quiet": self.quiet,
            "lang": self.lang,
            "token_budget": self.token_budget,
            "rate_limit_per_min": self.rate_limit_per_min,
            "fallback_url": self.fallback_url,
            "services": self.services,
        })
    }

    pub fn edit_in_editor(&self) -> Result<()> {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
        let mut cmd = std::process::Command::new(editor);
        cmd.arg(&self.config_path);
        cmd.status()?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if !url::Url::parse(&self.base_url).is_ok() {
            return Err(anyhow::anyhow!("Invalid base_url: {}", self.base_url));
        }

        if !self.allow_remote && !is_local_url(&self.base_url) {
            return Err(anyhow::anyhow!("Non-local endpoint not allowed (Rule 4): {}", self.base_url));
        }

        if !(0.0..=2.0).contains(&self.temperature) {
            return Err(anyhow::anyhow!("Temperature must be between 0.0 and 2.0"));
        }

        if self.max_tokens < 1 || self.max_tokens > 32768 {
            return Err(anyhow::anyhow!("max_tokens must be between 1 and 32768"));
        }

        Ok(())
    }
}

fn is_local_url(url: &str) -> bool {
    let host = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();

    let local_hosts = ["localhost", "127.0.0.1", "::1", "0.0.0.0"];
    if local_hosts.contains(&host.as_str()) || host.ends_with(".local") {
        return true;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback() || ip.is_private();
    }

    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub requests: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub token_budget: i64,
    pub budget_used_pct: f64,
    pub estimated_cost_usd: f64,
    pub estimated_cost_saved_usd: f64,
    pub rate_limit_per_min: i32,
    pub lang: String,
}

impl Config {
    pub fn get_cost_summary(&self, prompt_tokens: u64, completion_tokens: u64, requests: u64) -> CostSummary {
        let total_tokens = prompt_tokens + completion_tokens;
        let budget_used_pct = if self.token_budget > 0 {
            (total_tokens as f64 / self.token_budget as f64) * 100.0
        } else { 0.0 };

        let saved = (prompt_tokens as f64 * 0.35 + completion_tokens as f64 * 1.00) / 1_000_000.0;

        CostSummary {
            requests,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            token_budget: self.token_budget,
            budget_used_pct,
            estimated_cost_usd: saved,
            estimated_cost_saved_usd: saved,
            rate_limit_per_min: self.rate_limit_per_min,
            lang: self.lang.clone(),
        }
    }
}