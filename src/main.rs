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
use crate::time_expr::parse_time;

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
    let config = Config::from_cli(&cli).map_err(|err| AppError::Usage(format!("{err:#}")))?;
    let client = DatadogClient::new(config).map_err(|err| AppError::Usage(format!("{err:#}")))?;

    let response = match cli.command {
        Command::Logs {
            query,
            from,
            to,
            limit,
            sort,
            cursor,
        } => {
            let now = Utc::now();
            let from_dt = parse_time(&from, now).map_err(|err| AppError::Usage(format!("{err:#}")))?;
            let to_dt = parse_time(&to, now).map_err(|err| AppError::Usage(format!("{err:#}")))?;

            if to_dt <= from_dt {
                return Err(AppError::Usage(
                    "Invalid logs time window: `to` must be greater than `from`.".to_string(),
                ));
            }

            client
                .query_logs(LogsQuery {
                    query,
                    from: from_dt.to_rfc3339(),
                    to: to_dt.to_rfc3339(),
                    limit,
                    sort,
                    cursor,
                })
                .await
                .map_err(AppError::from)?
        }
        Command::Metrics { query, from, to } => {
            let now = Utc::now();
            let from_dt =
                parse_time(&from, now).map_err(|err| AppError::Usage(format!("{err:#}")))?;
            let to_dt =
                parse_time(&to, now).map_err(|err| AppError::Usage(format!("{err:#}")))?;

            if to_dt <= from_dt {
                return Err(AppError::Usage(
                    "Invalid metrics time window: `to` must be greater than `from`.".to_string(),
                ));
            }

            client
                .query_metrics(&query, from_dt.timestamp(), to_dt.timestamp())
                .await
                .map_err(AppError::from)?
        }
        Command::Events {
            query,
            from,
            to,
            limit,
            sort,
        } => {
            let now = Utc::now();
            let from_dt = parse_time(&from, now).map_err(|err| AppError::Usage(format!("{err:#}")))?;
            let to_dt = parse_time(&to, now).map_err(|err| AppError::Usage(format!("{err:#}")))?;

            if to_dt <= from_dt {
                return Err(AppError::Usage(
                    "Invalid events time window: `to` must be greater than `from`.".to_string(),
                ));
            }

            client
                .query_events(query, from_dt.to_rfc3339(), to_dt.to_rfc3339(), limit, sort)
                .await
                .map_err(AppError::from)?
        }
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
                .map_err(|err| AppError::Usage(format!("{err:#}")))?;
            Ok(Some(json))
        }
        (None, Some(path)) => {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed reading raw body file `{}`", path.display()))
                .map_err(|err| AppError::Usage(format!("{err:#}")))?;
            let json = serde_json::from_str::<Value>(&contents)
                .with_context(|| format!("Invalid JSON in raw body file `{}`", path.display()))
                .map_err(|err| AppError::Usage(format!("{err:#}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_query_params() {
        let params = vec!["key=value".to_string(), "foo=bar=baz".to_string()];
        let parsed = parse_query_params(&params).unwrap();
        assert_eq!(
            parsed,
            vec![
                ("key".to_string(), "value".to_string()),
                ("foo".to_string(), "bar=baz".to_string())
            ]
        );

        let invalid_no_eq = vec!["keyvalue".to_string()];
        assert!(parse_query_params(&invalid_no_eq).is_err());

        let invalid_empty_key = vec!["=value".to_string()];
        assert!(parse_query_params(&invalid_empty_key).is_err());
    }

    #[test]
    fn test_parse_raw_body() {
        // test valid string
        let body = Some(r#"{"key":"value"}"#.to_string());
        let parsed = parse_raw_body(body, None).unwrap();
        assert_eq!(parsed, Some(serde_json::json!({"key":"value"})));

        // test both
        let both_err = parse_raw_body(Some("{}".to_string()), Some(PathBuf::from("file.json")));
        assert!(both_err.is_err());

        // test invalid string
        let invalid_body = parse_raw_body(Some("{".to_string()), None);
        assert!(invalid_body.is_err());
    }
}
