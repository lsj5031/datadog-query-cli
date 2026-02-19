use anyhow::{Context, Result, anyhow};
use reqwest::{Method, Url};
use serde_json::{Value, json};

use crate::config::Config;

pub struct DatadogClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    app_key: String,
}

pub struct LogsQuery {
    pub query: String,
    pub from: String,
    pub to: String,
    pub limit: u32,
    pub sort: String,
    pub cursor: Option<String>,
}

impl DatadogClient {
    pub fn new(config: Config) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: config.base_url,
            api_key: config.api_key,
            app_key: config.app_key,
        }
    }

    pub async fn query_logs(&self, query: LogsQuery) -> Result<Value> {
        let sort = match query.sort.to_ascii_lowercase().as_str() {
            "asc" => "timestamp",
            "desc" => "-timestamp",
            other => {
                return Err(anyhow!(
                    "Invalid sort `{other}`. Use `asc` or `desc` for logs queries."
                ));
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

    pub async fn query_metrics(&self, query: &str, from: i64, to: i64) -> Result<Value> {
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
    ) -> Result<Value> {
        let sort = match sort.to_ascii_lowercase().as_str() {
            "asc" => "timestamp",
            "desc" => "-timestamp",
            other => {
                return Err(anyhow!(
                    "Invalid sort `{other}`. Use `asc` or `desc` for events queries."
                ));
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
    ) -> Result<Value> {
        let method =
            Method::from_bytes(method.as_bytes()).context("Invalid HTTP method for raw query.")?;
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
    ) -> Result<Value> {
        let mut url = self.resolve_url(path)?;
        if let Some(pairs) = params {
            {
                let mut query = url.query_pairs_mut();
                for (key, value) in pairs {
                    query.append_pair(&key, &value);
                }
            }
        }

        let mut request = self
            .http
            .request(method, url)
            .header("DD-API-KEY", &self.api_key)
            .header("DD-APPLICATION-KEY", &self.app_key)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        if let Some(payload) = body {
            request = request.json(&payload);
        }

        let response = request
            .send()
            .await
            .context("Datadog API request failed.")?;
        let status = response.status();
        let text = response
            .text()
            .await
            .context("Failed to read Datadog API response body.")?;

        if !status.is_success() {
            return Err(anyhow!(
                "Datadog API returned {}: {}",
                status.as_u16(),
                text
            ));
        }

        if text.trim().is_empty() {
            return Ok(json!({}));
        }

        match serde_json::from_str::<Value>(&text) {
            Ok(value) => Ok(value),
            Err(_) => Ok(json!({ "raw": text })),
        }
    }

    fn resolve_url(&self, path: &str) -> Result<Url> {
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
}
