use std::env;

use anyhow::{Context, Result, anyhow};

use crate::cli::Cli;

pub struct Config {
    pub api_key: String,
    pub app_key: String,
    pub base_url: String,
    pub retry: RetryConfig,
    pub timeout_seconds: u64,
}

pub struct RetryConfig {
    pub max_retries: u32,
    pub backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub retry_rate_limit: bool,
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let api_key = cli
            .api_key
            .clone()
            .or_else(|| env::var("DD_API_KEY").ok())
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .context("Missing Datadog API key. Set --api-key or DD_API_KEY.")?;

        let app_key = cli
            .app_key
            .clone()
            .or_else(|| env::var("DD_APP_KEY").ok())
            .or_else(|| env::var("DD_APPLICATION_KEY").ok())
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .context(
                "Missing Datadog application key. Set --app-key or DD_APP_KEY (or DD_APPLICATION_KEY).",
            )?;

        let site = cli
            .site
            .clone()
            .or_else(|| env::var("DD_SITE").ok())
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
        })
    }
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
