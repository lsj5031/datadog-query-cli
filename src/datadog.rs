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
    retry: RetryConfig,
    retry_timeout: Duration,
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
        retried: u32,
        rate_limit_info: Option<RateLimitInfo>,
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

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub remaining: Option<u64>,
    pub limit: Option<u64>,
    pub reset: Option<u64>,
    pub period: Option<u64>,
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
                retried,
                ..
            } => {
                if let Some(delay) = retry_after_ms {
                    write!(
                        f,
                        "Datadog rate limited request (429, retried={retried}, retry_after_ms={delay}): {body}"
                    )
                } else {
                    write!(f, "Datadog rate limited request (429, retried={retried}): {body}")
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
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "DD-API-KEY",
            reqwest::header::HeaderValue::from_str(&config.api_key)
                .context("Invalid characters in Datadog API key")?,
        );
        headers.insert(
            "DD-APPLICATION-KEY",
            reqwest::header::HeaderValue::from_str(&config.app_key)
                .context("Invalid characters in Datadog application key")?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            http,
            base_url: config.base_url.clone(),
            retry: config.retry,
            retry_timeout: Duration::from_secs(config.retry_timeout_seconds),
        })
    }

    pub fn base_host(&self) -> &str {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
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
        let method = Method::from_bytes(method.to_ascii_uppercase().as_bytes())
            .context("Invalid HTTP method for raw query.")
            .map_err(|err| DatadogError::InvalidRequest(format!("{err:#}")))?;
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
        let start = std::time::Instant::now();

        loop {
            if self.retry_timeout > Duration::ZERO
                && attempt > 0
                && start.elapsed() >= self.retry_timeout
            {
                tracing::warn!(
                    attempt = attempt,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    timeout_ms = self.retry_timeout.as_millis() as u64,
                    "Retry timeout exceeded, stopping retries"
                );
                break Err(DatadogError::Retryable {
                    status: None,
                    message: format!(
                        "Retry timeout of {}s exceeded after {} attempt(s)",
                        self.retry_timeout.as_secs(),
                        attempt
                    ),
                });
            }

            let mut url = self
                .resolve_url(path)
                .map_err(|err| DatadogError::InvalidRequest(format!("{err:#}")))?;
            if let Some(pairs) = &params {
                let mut query = url.query_pairs_mut();
                for (key, value) in pairs {
                    query.append_pair(key, value);
                }
            }

            let mut request = self.http.request(method.clone(), url);

            if let Some(payload) = &body {
                request = request.json(payload);
            }

            let response = match request.send().await {
                Ok(response) => response,
                Err(err) => {
                    if is_retryable_transport_error(&err) && attempt < self.retry.max_retries {
                        let backoff = self.backoff_ms(attempt);
                        tracing::info!(
                            attempt = attempt + 1,
                            max_retries = self.retry.max_retries,
                            sleep_ms = backoff,
                            reason = "transport_error",
                            error = %err,
                            "Retrying request"
                        );
                        self.sleep_before_retry(attempt, None).await;
                        attempt += 1;
                        continue;
                    }

                    if is_retryable_transport_error(&err) {
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = self.retry.max_retries,
                            error = %err,
                            "Request failed, no retries remaining"
                        );
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
            let headers = response.headers().clone();
            let retry_after_ms = parse_retry_after_ms(&headers);
            let rate_limit_info = parse_rate_limit_headers(&headers);
            let text = match response.text().await {
                Ok(text) => text,
                Err(err) => {
                    if attempt < self.retry.max_retries {
                        let backoff = self.backoff_ms(attempt);
                        tracing::info!(
                            attempt = attempt + 1,
                            max_retries = self.retry.max_retries,
                            sleep_ms = backoff,
                            reason = "body_read_error",
                            "Retrying request"
                        );
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

            let err_body = truncate_for_error(&text);
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                return Err(DatadogError::Auth {
                    status: status.as_u16(),
                    body: err_body,
                });
            }

            if status == StatusCode::TOO_MANY_REQUESTS {
                if let Some(ref info) = rate_limit_info {
                    tracing::info!(
                        remaining = info.remaining,
                        limit = info.limit,
                        reset = info.reset,
                        period = info.period,
                        "Rate limit headers from Datadog"
                    );
                }

                if self.retry.retry_rate_limit && attempt < self.retry.max_retries {
                    let sleep_ms = compute_429_sleep_ms(
                        retry_after_ms,
                        rate_limit_info.as_ref(),
                        self.backoff_ms(attempt),
                    );
                    tracing::info!(
                        attempt = attempt + 1,
                        max_retries = self.retry.max_retries,
                        sleep_ms,
                        reason = "rate_limit",
                        status = 429,
                        "Retrying request"
                    );
                    sleep(Duration::from_millis(sleep_ms)).await;
                    attempt += 1;
                    continue;
                }

                tracing::warn!(
                    attempt = attempt + 1,
                    max_retries = self.retry.max_retries,
                    retry_rate_limit = self.retry.retry_rate_limit,
                    "Rate limited, no retries remaining"
                );
                return Err(DatadogError::RateLimited {
                    body: err_body,
                    retry_after_ms,
                    retried: attempt,
                    rate_limit_info,
                });
            }

            if is_retryable_status(status) {
                if attempt < self.retry.max_retries {
                    let backoff = self.backoff_ms(attempt);
                    tracing::info!(
                        attempt = attempt + 1,
                        max_retries = self.retry.max_retries,
                        sleep_ms = backoff,
                        reason = "server_error",
                        status = status.as_u16(),
                        "Retrying request"
                    );
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
                        err_body
                    ),
                });
            }

            return Err(DatadogError::Api {
                status: status.as_u16(),
                body: err_body,
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

fn compute_429_sleep_ms(
    retry_after_ms: Option<u64>,
    rate_limit_info: Option<&RateLimitInfo>,
    backoff_ms: u64,
) -> u64 {
    if let Some(ms) = retry_after_ms {
        return ms;
    }
    if let Some(info) = rate_limit_info
        && let Some(reset_secs) = info.reset
    {
        return reset_secs.saturating_mul(1_000);
    }
    backoff_ms
}

fn truncate_for_error(text: &str) -> String {
    let trimmed = text.trim();
    const MAX_ERROR_BODY_BYTES: usize = 2_048;
    if trimmed.len() <= MAX_ERROR_BODY_BYTES {
        return trimmed.to_string();
    }

    let mut end = MAX_ERROR_BODY_BYTES;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}...(truncated)", &trimmed[..end])
}

fn parse_retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let value = headers.get("Retry-After")?;
    let as_text = value.to_str().ok()?.trim();

    // Try integer seconds first
    if let Ok(seconds) = as_text.parse::<u64>() {
        return Some(seconds.saturating_mul(1_000));
    }

    // Try HTTP-date format (RFC 2822 / IMF-fixdate)
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(as_text) {
        let now = chrono::Utc::now();
        let remaining = dt.with_timezone(&chrono::Utc) - now;
        let ms = remaining.num_milliseconds().max(0) as u64;
        return Some(ms);
    }

    None
}

fn parse_rate_limit_headers(headers: &reqwest::header::HeaderMap) -> Option<RateLimitInfo> {
    let remaining = headers
        .get("X-RateLimit-Remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    let limit = headers
        .get("X-RateLimit-Limit")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    let reset = headers
        .get("X-RateLimit-Reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    let period = headers
        .get("X-RateLimit-Period")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());

    if remaining.is_some() || limit.is_some() || reset.is_some() || period.is_some() {
        Some(RateLimitInfo {
            remaining,
            limit,
            reset,
            period,
        })
    } else {
        None
    }
}

fn is_retryable_transport_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_body() || err.is_request()
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT || status.is_server_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> DatadogClient {
        let config = Config {
            api_key: "test-key".to_string(),
            app_key: "test-app-key".to_string(),
            base_url: "https://api.datadoghq.com".to_string(),
            retry: RetryConfig {
                max_retries: 3,
                backoff_ms: 10,
                max_backoff_ms: 100,
                retry_rate_limit: true,
            },
            timeout_seconds: 5,
            retry_timeout_seconds: 5,
        };
        DatadogClient::new(config).unwrap()
    }

    #[test]
    fn truncate_for_error_keeps_short_text() {
        let text = "short body";
        assert_eq!(truncate_for_error(text), text);
    }

    #[test]
    fn truncate_for_error_handles_multibyte_utf8_without_panicking() {
        let text = "x".repeat(2_047) + "étail";
        let truncated = truncate_for_error(&text);
        let suffix = "...(truncated)";
        let prefix = truncated.strip_suffix(suffix).unwrap();

        assert!(truncated.ends_with(suffix));
        assert!(prefix.len() <= 2_048);
    }

    #[test]
    fn test_truncate_for_error_boundary() {
        let mut text = String::new();
        for _ in 0..1000 {
            text.push('🦀');
        }

        let mut text2 = String::new();
        text2.push('a');
        for _ in 0..1000 {
            text2.push('🦀');
        }

        let truncated = truncate_for_error(&text2);
        assert!(!truncated.is_empty());
    }

    #[test]
    fn test_backoff_exponential_capped() {
        let client = test_client();
        // backoff_ms: 10, max: 100
        // attempt 0: 10 * 1 = 10
        assert_eq!(client.backoff_ms(0), 10);
        // attempt 1: 10 * 2 = 20
        assert_eq!(client.backoff_ms(1), 20);
        // attempt 2: 10 * 4 = 40
        assert_eq!(client.backoff_ms(2), 40);
        // attempt 3: 10 * 8 = 80
        assert_eq!(client.backoff_ms(3), 80);
        // attempt 4: 10 * 16 = 160 -> capped at 100
        assert_eq!(client.backoff_ms(4), 100);
        // attempt 10: still capped
        assert_eq!(client.backoff_ms(10), 100);
    }

    #[test]
    fn test_retry_after_seconds_header() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Retry-After", "30".parse().unwrap());
        let result = parse_retry_after_ms(&headers);
        assert_eq!(result, Some(30_000));
    }

    #[test]
    fn test_retry_after_missing_header() {
        let headers = reqwest::header::HeaderMap::new();
        let result = parse_retry_after_ms(&headers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_retry_after_http_date() {
        let mut headers = reqwest::header::HeaderMap::new();
        // Set a date 45 seconds in the future
        let future = chrono::Utc::now() + chrono::Duration::seconds(45);
        let date_str = future.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        headers.insert("Retry-After", date_str.parse().unwrap());
        let result = parse_retry_after_ms(&headers);
        assert!(result.is_some());
        let ms = result.unwrap();
        // Should be roughly 45 seconds, allow 2s tolerance for test execution
        assert!(ms > 43_000, "Expected >43000ms, got {ms}");
        assert!(ms <= 45_000, "Expected <=45000ms, got {ms}");
    }

    #[test]
    fn test_retry_after_http_date_past() {
        let mut headers = reqwest::header::HeaderMap::new();
        // Date in the past -> should give 0 or near-zero
        let past = chrono::Utc::now() - chrono::Duration::seconds(30);
        let date_str = past.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        headers.insert("Retry-After", date_str.parse().unwrap());
        let result = parse_retry_after_ms(&headers);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_retry_after_garbage_value() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Retry-After", "not-a-valid-value".parse().unwrap());
        let result = parse_retry_after_ms(&headers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_rate_limit_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-RateLimit-Limit", "300".parse().unwrap());
        headers.insert("X-RateLimit-Remaining", "5".parse().unwrap());
        headers.insert("X-RateLimit-Reset", "1700000000".parse().unwrap());
        headers.insert("X-RateLimit-Period", "60".parse().unwrap());

        let info = parse_rate_limit_headers(&headers).unwrap();
        assert_eq!(info.limit, Some(300));
        assert_eq!(info.remaining, Some(5));
        assert_eq!(info.reset, Some(1700000000));
        assert_eq!(info.period, Some(60));
    }

    #[test]
    fn test_parse_rate_limit_headers_partial() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-RateLimit-Remaining", "10".parse().unwrap());

        let info = parse_rate_limit_headers(&headers).unwrap();
        assert_eq!(info.remaining, Some(10));
        assert_eq!(info.limit, None);
        assert_eq!(info.reset, None);
        assert_eq!(info.period, None);
    }

    #[test]
    fn test_parse_rate_limit_headers_empty() {
        let headers = reqwest::header::HeaderMap::new();
        let result = parse_rate_limit_headers(&headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_base_host() {
        let client = test_client();
        assert_eq!(client.base_host(), "api.datadoghq.com");
    }

    #[test]
    fn test_base_host_custom_url() {
        let config = Config {
            api_key: "test-key".to_string(),
            app_key: "test-app-key".to_string(),
            base_url: "http://localhost:8080".to_string(),
            retry: RetryConfig {
                max_retries: 3,
                backoff_ms: 10,
                max_backoff_ms: 100,
                retry_rate_limit: true,
            },
            timeout_seconds: 5,
            retry_timeout_seconds: 5,
        };
        let client = DatadogClient::new(config).unwrap();
        assert_eq!(client.base_host(), "localhost:8080");
    }

    // --- Bug 1 tests: compute_429_sleep_ms uses X-RateLimit-Reset fallback ---

    #[test]
    fn test_compute_429_sleep_prefers_retry_after_over_reset() {
        // When both Retry-After and X-RateLimit-Reset are present, Retry-After wins
        let retry_after_ms = Some(5000u64);
        let rate_limit_info = Some(RateLimitInfo {
            remaining: Some(0),
            limit: Some(2),
            reset: Some(14),
            period: Some(30),
        });
        let backoff = 250u64;
        let result = compute_429_sleep_ms(retry_after_ms, rate_limit_info.as_ref(), backoff);
        assert_eq!(result, 5000);
    }

    #[test]
    fn test_compute_429_sleep_uses_reset_when_no_retry_after() {
        // When Retry-After is missing, use X-RateLimit-Reset (seconds -> ms)
        let retry_after_ms = None;
        let rate_limit_info = Some(RateLimitInfo {
            remaining: Some(0),
            limit: Some(2),
            reset: Some(14),
            period: Some(30),
        });
        let backoff = 250u64;
        let result = compute_429_sleep_ms(retry_after_ms, rate_limit_info.as_ref(), backoff);
        assert_eq!(result, 14_000);
    }

    #[test]
    fn test_compute_429_sleep_uses_backoff_when_neither() {
        // When both Retry-After and X-RateLimit-Reset are missing, fall back to backoff
        let retry_after_ms = None;
        let rate_limit_info = Some(RateLimitInfo {
            remaining: Some(0),
            limit: Some(2),
            reset: None,
            period: Some(30),
        });
        let backoff = 500u64;
        let result = compute_429_sleep_ms(retry_after_ms, rate_limit_info.as_ref(), backoff);
        assert_eq!(result, 500);
    }

    #[test]
    fn test_compute_429_sleep_uses_backoff_when_no_rate_limit_info() {
        let retry_after_ms = None;
        let rate_limit_info: Option<RateLimitInfo> = None;
        let backoff = 750u64;
        let result = compute_429_sleep_ms(retry_after_ms, rate_limit_info.as_ref(), backoff);
        assert_eq!(result, 750);
    }

    // --- Bug 3 tests: truncate_for_error trims whitespace ---

    #[test]
    fn test_truncate_for_error_trims_whitespace() {
        let text = "\n  {\"error\": \"too many requests\"}\n  ";
        assert_eq!(truncate_for_error(text), "{\"error\": \"too many requests\"}");
    }

    #[test]
    fn test_truncate_for_error_trims_and_truncates() {
        let inner = "x".repeat(2_060);
        let text = format!("\n{inner}\n");
        let result = truncate_for_error(&text);
        assert!(!result.starts_with('\n'));
        assert!(result.ends_with("...(truncated)"));
        assert!(result.len() <= 2_048 + "...(truncated)".len());
    }
}

