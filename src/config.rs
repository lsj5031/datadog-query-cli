use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::cli::Cli;

pub struct Config {
    pub api_key: String,
    pub app_key: String,
    pub base_url: String,
    pub retry: RetryConfig,
    pub timeout_seconds: u64,
    pub retry_timeout_seconds: u64,
}

pub struct RetryConfig {
    pub max_retries: u32,
    pub backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub retry_rate_limit: bool,
}

#[derive(Deserialize, Default)]
struct TomlConfig {
    api_key: Option<String>,
    app_key: Option<String>,
    site: Option<String>,
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let toml_config = load_toml_config();

        let api_key = cli
            .api_key
            .clone()
            .or_else(|| env::var("DD_API_KEY").ok())
            .or_else(|| toml_config.api_key.clone())
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .context("Missing Datadog API key. Set --api-key, DD_API_KEY, or api_key in config.toml.")?;

        let app_key = cli
            .app_key
            .clone()
            .or_else(|| env::var("DD_APP_KEY").ok())
            .or_else(|| env::var("DD_APPLICATION_KEY").ok())
            .or_else(|| toml_config.app_key.clone())
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .context(
                "Missing Datadog application key. Set --app-key, DD_APP_KEY, or app_key in config.toml.",
            )?;

        let site = cli
            .site
            .clone()
            .or_else(|| env::var("DD_SITE").ok())
            .or_else(|| toml_config.site.clone())
            .unwrap_or_else(|| "datadoghq.com".to_string());

        let base_url = normalize_base_url(&site)?;

        if cli.retry_backoff_ms == 0 {
            return Err(anyhow!("--retry-backoff-ms must be greater than 0."));
        }
        if cli.retry_max_backoff_ms < cli.retry_backoff_ms {
            return Err(anyhow!(
                "--retry-max-backoff-ms must be greater than or equal to --retry-backoff-ms."
            ));
        }
        if cli.timeout_seconds == 0 {
            return Err(anyhow!("--timeout-seconds must be greater than 0."));
        }

        Ok(Self {
            api_key,
            app_key,
            base_url,
            retry: RetryConfig {
                max_retries: cli.retries,
                backoff_ms: cli.retry_backoff_ms,
                max_backoff_ms: cli.retry_max_backoff_ms,
                retry_rate_limit: cli.retry_rate_limit,
            },
            timeout_seconds: cli.timeout_seconds,
            retry_timeout_seconds: cli.retry_timeout_seconds,
        })
    }
}

fn load_toml_config() -> TomlConfig {
    let config_path = find_config_file();
    
    if let Some(path) = config_path
        && let Ok(contents) = fs::read_to_string(&path)
    {
        match toml::from_str::<TomlConfig>(&contents) {
            Ok(config) => return config,
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "Failed to parse config file, ignoring"
                );
            }
        }
    }
    
    TomlConfig::default()
}

fn find_config_file() -> Option<PathBuf> {
    // 1. Check local directory first (.ddq.toml or datadog.toml)
    let local_paths = [PathBuf::from(".ddq.toml"), PathBuf::from("datadog.toml")];
    for path in &local_paths {
        if path.exists() {
            return Some(path.clone());
        }
    }

    // 2. Fall back to OS standard config directory (~/.config/ddq/config.toml)
    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "ddq") {
        let config_file = proj_dirs.config_dir().join("config.toml");
        if config_file.exists() {
            return Some(config_file);
        }
    }

    None
}

fn normalize_base_url(site: &str) -> Result<String> {
    let cleaned = site.trim().trim_end_matches('/');
    if cleaned.is_empty() {
        return Err(anyhow!("Datadog site value is empty."));
    }

    if cleaned.starts_with("http://") || cleaned.starts_with("https://") {
        return Ok(cleaned.to_string());
    }

    if cleaned.starts_with("api.") {
        return Ok(format!("https://{cleaned}"));
    }

    Ok(format!("https://api.{cleaned}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_base_url() {
        assert_eq!(
            normalize_base_url("datadoghq.com").unwrap(),
            "https://api.datadoghq.com"
        );
        assert_eq!(
            normalize_base_url("us3.datadoghq.com").unwrap(),
            "https://api.us3.datadoghq.com"
        );
        assert_eq!(
            normalize_base_url("api.datadoghq.eu").unwrap(),
            "https://api.datadoghq.eu"
        );
        assert_eq!(
            normalize_base_url("https://custom.datadog.com").unwrap(),
            "https://custom.datadog.com"
        );
        assert_eq!(
            normalize_base_url("http://localhost:8080/").unwrap(),
            "http://localhost:8080"
        );
        assert!(normalize_base_url("   /   ").is_err());
    }
}
