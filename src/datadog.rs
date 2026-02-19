use std::fmt::{Display, Formatter};
use std::time::Duration;

use anyhow::Context;
use reqwest::{Method, StatusCode, Url};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::config::{Config, RetryConfig};

pub struct DatadogClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    app_key: String,
    retry: RetryConfig,
    timeout_seconds: u64,
}

pub struct LogsQuery {
    pub query: String,
    pub from: String,
    pub to: String,
    pub limit: u32,
    pub sort: String,
    pub cursor: Option<String>,
}

#[derive(Debug)]
pub enum DatadogError {
    InvalidRequest(String),
    Auth {
        status: u16,
        body: String,
    },
    RateLimited {
        body: String,
        retry_after_ms: Option<u64>,
    },
    Retryable {
        status: Option<u16>,
        message: String,
    },
    Api {
        status: u16,
        body: String,
    },
}

impl Display for DatadogError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "{message}"),
            Self::Auth { status, body } => {
                write!(f, "Datadog auth error ({status}): {body}")
            }
            Self::RateLimited {
                body,
                retry_after_ms,
            } => {
                if let Some(delay) = retry_after_ms {
                    write!(
                        f,
                        "Datadog rate limited request (429, retry_after_ms={delay}): {body}"
                    )
                } else {
                    write!(f, "Datadog rate limited request (429): {body}")
                }
            }
            Self::Retryable { status, message } => {
                if let Some(status) = status {
                    write!(f, "Datadog retryable upstream error ({status}): {message}")
                } else {
                    write!(f, "Datadog retryable transport error: {message}")
                }
            }
            Self::Api { status, body } => {
                write!(f, "Datadog API error ({status}): {body}")
            }
        }
    }
}

impl std::error::Error for DatadogError {}

impl DatadogClient {
    pub fn new(config: Config) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: config.base_url,
            api_key: config.api_key,
            app_key: config.app_key,
            retry: config.retry,
            timeout_seconds: config.timeout_seconds,
        }
    }

    pub async fn query_logs(&self, query: LogsQuery) -> Result<Value, DatadogError> {
        let sort = match query.sort.to_ascii_lowercase().as_str() {
            "asc" => "timestamp",
            "desc" => "-timestamp",
            other => {
                return Err(DatadogError::InvalidRequest(format!(
                    "Invalid sort `{other}`. Use `asc` or `desc` for logs queries."
                )));
            }
        };

        let mut page = json!({ "limit": query.limit });
        if let Some(cursor) = query.cursor {
            page["cursor"] = json!(cursor);
        }

        let body = json!({
            "filter": {
                "query": query.query,
                "from": query.from,
                "to": query.to
            },
            "sort": sort,
            "page": page
        });

        self.send_json(Method::POST, "/api/v2/logs/events/search", None, Some(body))
            .await
    }

    pub async fn query_metrics(
        &self,
        query: &str,
        from: i64,
        to: i64,
    ) -> Result<Value, DatadogError> {
        let params = vec![
            ("query".to_string(), query.to_string()),
            ("from".to_string(), from.to_string()),
            ("to".to_string(), to.to_string()),
        ];

        self.send_json(Method::GET, "/api/v1/query", Some(params), None)
            .await
    }

    pub async fn query_events(
        &self,
        query: Option<String>,
        from: String,
        to: String,
        limit: u32,
        sort: String,
    ) -> Result<Value, DatadogError> {
        let sort = match sort.to_ascii_lowercase().as_str() {
            "asc" => "timestamp",
            "desc" => "-timestamp",
            other => {
                return Err(DatadogError::InvalidRequest(format!(
                    "Invalid sort `{other}`. Use `asc` or `desc` for events queries."
                )));
            }
        };

        let mut params = vec![
            ("filter[from]".to_string(), from),
            ("filter[to]".to_string(), to),
            ("page[limit]".to_string(), limit.to_string()),
            ("sort".to_string(), sort.to_string()),
        ];
        if let Some(query) = query {
            params.push(("filter[query]".to_string(), query));
        }

        self.send_json(Method::GET, "/api/v2/events", Some(params), None)
            .await
    }

    pub async fn raw(
        &self,
        method: &str,
        path: &str,
        params: Vec<(String, String)>,
        body: Option<Value>,
    ) -> Result<Value, DatadogError> {
        let method = Method::from_bytes(method.as_bytes())
            .context("Invalid HTTP method for raw query.")
            .map_err(|err| DatadogError::InvalidRequest(err.to_string()))?;
        let params = if params.is_empty() {
            None
        } else {
            Some(params)
        };
        self.send_json(method, path, params, body).await
    }

    async fn send_json(
        &self,
        method: Method,
        path: &str,
        params: Option<Vec<(String, String)>>,
        body: Option<Value>,
    ) -> Result<Value, DatadogError> {
        let mut attempt: u32 = 0;

        loop {
            let mut url = self
                .resolve_url(path)
                .map_err(|err| DatadogError::InvalidRequest(err.to_string()))?;
            if let Some(pairs) = &params {
                let mut query = url.query_pairs_mut();
                for (key, value) in pairs {
                    query.append_pair(key, value);
                }
            }

            let mut request = self
                .http
                .request(method.clone(), url)
                .header("DD-API-KEY", &self.api_key)
                .header("DD-APPLICATION-KEY", &self.app_key)
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .timeout(Duration::from_secs(self.timeout_seconds));

            if let Some(payload) = &body {
                request = request.json(payload);
            }

            let response = match request.send().await {
                Ok(response) => response,
                Err(err) => {
                    if is_retryable_transport_error(&err) && attempt < self.retry.max_retries {
                        self.sleep_before_retry(attempt, None).await;
                        attempt += 1;
                        continue;
                    }

                    if is_retryable_transport_error(&err) {
                        return Err(DatadogError::Retryable {
                            status: None,
                            message: format!(
                                "Datadog request failed after {} attempt(s): {}",
                                attempt + 1,
                                err
                            ),
                        });
                    }

                    return Err(DatadogError::InvalidRequest(format!(
                        "Datadog request setup failed: {err}"
                    )));
                }
            };

            let status = response.status();
            let retry_after_ms = parse_retry_after_ms(response.headers());
            let text = match response.text().await {
                Ok(text) => text,
                Err(err) => {
                    if attempt < self.retry.max_retries {
                        self.sleep_before_retry(attempt, None).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(DatadogError::Retryable {
                        status: Some(status.as_u16()),
                        message: format!(
                            "Failed to read Datadog response after {} attempt(s): {}",
                            attempt + 1,
                            err
                        ),
                    });
                }
            };

            if status.is_success() {
                if text.trim().is_empty() {
                    return Ok(json!({}));
                }

                return match serde_json::from_str::<Value>(&text) {
                    Ok(value) => Ok(value),
                    Err(_) => Ok(json!({ "raw": text })),
                };
            }

            let body = truncate_for_error(&text);
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                return Err(DatadogError::Auth {
                    status: status.as_u16(),
                    body,
                });
            }

            if status == StatusCode::TOO_MANY_REQUESTS {
                if self.retry.retry_rate_limit && attempt < self.retry.max_retries {
                    self.sleep_before_retry(attempt, retry_after_ms).await;
                    attempt += 1;
                    continue;
                }
                return Err(DatadogError::RateLimited {
                    body,
                    retry_after_ms,
                });
            }

            if is_retryable_status(status) {
                if attempt < self.retry.max_retries {
                    self.sleep_before_retry(attempt, None).await;
                    attempt += 1;
                    continue;
                }
                return Err(DatadogError::Retryable {
                    status: Some(status.as_u16()),
                    message: format!(
                        "Datadog API returned {} after {} attempt(s): {}",
                        status.as_u16(),
                        attempt + 1,
                        body
                    ),
                });
            }

            return Err(DatadogError::Api {
                status: status.as_u16(),
                body,
            });
        }
    }

    fn resolve_url(&self, path: &str) -> anyhow::Result<Url> {
        if path.starts_with("http://") || path.starts_with("https://") {
            return Url::parse(path).context("Invalid raw URL.");
        }

        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let url = format!("{}{}", self.base_url, normalized_path);
        Url::parse(&url).with_context(|| format!("Invalid Datadog URL built from `{url}`"))
    }

    async fn sleep_before_retry(&self, attempt: u32, retry_after_ms: Option<u64>) {
        let delay_ms = retry_after_ms.unwrap_or_else(|| self.backoff_ms(attempt));
        sleep(Duration::from_millis(delay_ms)).await;
    }

    fn backoff_ms(&self, attempt: u32) -> u64 {
        let multiplier = 1u64 << attempt.min(16);
        self.retry
            .backoff_ms
            .saturating_mul(multiplier)
            .min(self.retry.max_backoff_ms)
    }
}

fn truncate_for_error(text: &str) -> String {
    const MAX_ERROR_BODY_BYTES: usize = 2_048;
    if text.len() <= MAX_ERROR_BODY_BYTES {
        return text.to_string();
    }
    format!("{}...(truncated)", &text[..MAX_ERROR_BODY_BYTES])
}

fn parse_retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let value = headers.get("Retry-After")?;
    let as_text = value.to_str().ok()?.trim();
    let seconds = as_text.parse::<u64>().ok()?;
    Some(seconds.saturating_mul(1_000))
}

fn is_retryable_transport_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_body() || err.is_request()
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT || status.is_server_error()
}
