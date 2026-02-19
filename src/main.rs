mod cli;
mod config;
mod datadog;
mod time_expr;

use std::fs;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use clap::Parser;
use serde_json::Value;

use crate::cli::{Cli, Command};
use crate::config::Config;
use crate::datadog::{DatadogClient, LogsQuery};
use crate::time_expr::parse_to_unix;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli)?;
    let client = DatadogClient::new(config);

    let response = match cli.command {
        Command::Logs {
            query,
            from,
            to,
            limit,
            sort,
            cursor,
        } => {
            client
                .query_logs(LogsQuery {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                    cursor,
                })
                .await?
        }
        Command::Metrics { query, from, to } => {
            let now = Utc::now();
            let from_unix = parse_to_unix(&from, now)?;
            let to_unix = parse_to_unix(&to, now)?;

            if to_unix <= from_unix {
                return Err(anyhow!(
                    "Invalid metrics time window: `to` must be greater than `from`."
                ));
            }

            client.query_metrics(&query, from_unix, to_unix).await?
        }
        Command::Events {
            query,
            from,
            to,
            limit,
            sort,
        } => client.query_events(query, from, to, limit, sort).await?,
        Command::Raw {
            method,
            path,
            query_params,
            body,
            body_file,
        } => {
            let params = parse_query_params(&query_params)?;
            let payload = parse_raw_body(body, body_file)?;
            client.raw(&method, &path, params, payload).await?
        }
    };

    print_json(response, cli.compact)?;
    Ok(())
}

fn parse_query_params(params: &[String]) -> Result<Vec<(String, String)>> {
    params
        .iter()
        .map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts
                .next()
                .ok_or_else(|| anyhow!("Invalid query param `{pair}`. Expected key=value."))?;
            if key.is_empty() {
                return Err(anyhow!("Query param key cannot be empty in `{pair}`."));
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

fn parse_raw_body(
    body: Option<String>,
    body_file: Option<std::path::PathBuf>,
) -> Result<Option<Value>> {
    match (body, body_file) {
        (Some(_), Some(_)) => Err(anyhow!(
            "Provide only one of --body or --body-file for raw requests."
        )),
        (Some(raw), None) => {
            let json = serde_json::from_str::<Value>(&raw)
                .context("Invalid JSON passed to --body for raw request.")?;
            Ok(Some(json))
        }
        (None, Some(path)) => {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed reading raw body file `{}`", path.display()))?;
            let json = serde_json::from_str::<Value>(&contents)
                .with_context(|| format!("Invalid JSON in raw body file `{}`", path.display()))?;
            Ok(Some(json))
        }
        (None, None) => Ok(None),
    }
}

fn print_json(value: Value, compact: bool) -> Result<()> {
    if compact {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}
