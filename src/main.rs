mod app_error;
mod cli;
mod config;
mod datadog;
mod time_expr;

use std::fs;

use anyhow::Context;
use chrono::Utc;
use clap::Parser;
use serde_json::Value;

use crate::app_error::AppError;
use crate::cli::{Cli, Command};
use crate::config::Config;
use crate::datadog::{DatadogClient, LogsQuery};
use crate::time_expr::parse_to_unix;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let compact = cli.compact_output();

    if let Err(err) = run(cli, compact).await {
        if print_json_stderr(err.to_json(), compact).is_err() {
            eprintln!(
                "{{\"error\":{{\"category\":\"internal\",\"exit_code\":1,\"message\":\"Failed serializing error output\"}}}}"
            );
            std::process::exit(1);
        }
        std::process::exit(err.exit_code());
    }
}

async fn run(cli: Cli, compact: bool) -> Result<(), AppError> {
    let config = Config::from_cli(&cli).map_err(|err| AppError::Usage(err.to_string()))?;
    let client = DatadogClient::new(config);

    let response = match cli.command {
        Command::Logs {
            query,
            from,
            to,
            limit,
            sort,
            cursor,
        } => client
            .query_logs(LogsQuery {
                query,
                from,
                to,
                limit,
                sort,
                cursor,
            })
            .await
            .map_err(AppError::from)?,
        Command::Metrics { query, from, to } => {
            let now = Utc::now();
            let from_unix =
                parse_to_unix(&from, now).map_err(|err| AppError::Usage(err.to_string()))?;
            let to_unix =
                parse_to_unix(&to, now).map_err(|err| AppError::Usage(err.to_string()))?;

            if to_unix <= from_unix {
                return Err(AppError::Usage(
                    "Invalid metrics time window: `to` must be greater than `from`.".to_string(),
                ));
            }

            client
                .query_metrics(&query, from_unix, to_unix)
                .await
                .map_err(AppError::from)?
        }
        Command::Events {
            query,
            from,
            to,
            limit,
            sort,
        } => client
            .query_events(query, from, to, limit, sort)
            .await
            .map_err(AppError::from)?,
        Command::Raw {
            method,
            path,
            query_params,
            body,
            body_file,
        } => {
            let params = parse_query_params(&query_params)?;
            let payload = parse_raw_body(body, body_file)?;
            client
                .raw(&method, &path, params, payload)
                .await
                .map_err(AppError::from)?
        }
    };

    print_json_stdout(response, compact).map_err(|err| AppError::Internal(err.to_string()))?;
    Ok(())
}

fn parse_query_params(params: &[String]) -> Result<Vec<(String, String)>, AppError> {
    params
        .iter()
        .map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().ok_or_else(|| {
                AppError::Usage(format!("Invalid query param `{pair}`. Expected key=value."))
            })?;
            if key.is_empty() {
                return Err(AppError::Usage(format!(
                    "Query param key cannot be empty in `{pair}`."
                )));
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

fn parse_raw_body(
    body: Option<String>,
    body_file: Option<std::path::PathBuf>,
) -> Result<Option<Value>, AppError> {
    match (body, body_file) {
        (Some(_), Some(_)) => Err(AppError::Usage(
            "Provide only one of --body or --body-file for raw requests.".to_string(),
        )),
        (Some(raw), None) => {
            let json = serde_json::from_str::<Value>(&raw)
                .context("Invalid JSON passed to --body for raw request.")
                .map_err(|err| AppError::Usage(err.to_string()))?;
            Ok(Some(json))
        }
        (None, Some(path)) => {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed reading raw body file `{}`", path.display()))
                .map_err(|err| AppError::Usage(err.to_string()))?;
            let json = serde_json::from_str::<Value>(&contents)
                .with_context(|| format!("Invalid JSON in raw body file `{}`", path.display()))
                .map_err(|err| AppError::Usage(err.to_string()))?;
            Ok(Some(json))
        }
        (None, None) => Ok(None),
    }
}

fn print_json_stdout(value: Value, compact: bool) -> Result<(), serde_json::Error> {
    if compact {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn print_json_stderr(value: Value, compact: bool) -> Result<(), serde_json::Error> {
    if compact {
        eprintln!("{}", serde_json::to_string(&value)?);
    } else {
        eprintln!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}
