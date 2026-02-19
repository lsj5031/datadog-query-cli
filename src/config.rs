use std::env;

use anyhow::{Context, Result, anyhow};

use crate::cli::Cli;

pub struct Config {
    pub api_key: String,
    pub app_key: String,
    pub base_url: String,
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let api_key = cli
            .api_key
            .clone()
            .or_else(|| env::var("DD_API_KEY").ok())
            .context("Missing Datadog API key. Set --api-key or DD_API_KEY.")?;

        let app_key = cli
            .app_key
            .clone()
            .or_else(|| env::var("DD_APP_KEY").ok())
            .or_else(|| env::var("DD_APPLICATION_KEY").ok())
            .context(
                "Missing Datadog application key. Set --app-key or DD_APP_KEY (or DD_APPLICATION_KEY).",
            )?;

        let site = cli
            .site
            .clone()
            .or_else(|| env::var("DD_SITE").ok())
            .unwrap_or_else(|| "datadoghq.com".to_string());

        let base_url = normalize_base_url(&site)?;
        Ok(Self {
            api_key,
            app_key,
            base_url,
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
