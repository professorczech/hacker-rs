// src/config.rs
use anyhow::{Context, Result};
use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};
use shellexpand;
use std::fs;
use std::path::PathBuf;

// --- ModelConfig struct ---
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelConfig {
    pub name: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

// --- AdvancedConfig struct ---
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AdvancedConfig {
    pub qwen_formatting: Option<bool>,
}

// --- AppConfig struct ---
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub model: ModelConfig,
    pub ollama_host: Option<String>,
    pub advanced: Option<AdvancedConfig>,
}

impl AppConfig {
    pub fn from_file(path: &str) -> Result<Self> {
        let expanded_path = shellexpand::tilde(path);
        // Now .context() should work because the Context trait is in scope
        let config_str = fs::read_to_string(expanded_path.as_ref())
            .context(format!("Failed to read config file: {}", path))?;
        let config: AppConfig = toml::from_str(&config_str)
            .context(format!("Failed to parse TOML from config file: {}", path))?;
        Ok(config)
    }

    pub fn default_path() -> PathBuf {
        ProjectDirs::from("rs", "professorczech", "hacker-rs")
            .map(|proj_dirs| proj_dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("config.toml")
    }

    pub fn generate_default_config() -> Result<()> {
        let default_path = Self::default_path();
        let default_dir = default_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid default config path parent"))?;

        std::fs::create_dir_all(default_dir)?;

        let default_config = AppConfig {
            model: ModelConfig {
                name: "phi4-mini:latest".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(1000),
            },
            ollama_host: Some("http://localhost:11434".to_string()),
            advanced: Some(AdvancedConfig {
                qwen_formatting: Some(true),
            }),
        };

        let toml = toml::to_string_pretty(&default_config)?;
        std::fs::write(&default_path, toml)?;
        Ok(())
    }
}