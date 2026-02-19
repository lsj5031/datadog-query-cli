use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "ddq",
    version,
    about = "Query Datadog APIs from local CLI without MCP"
)]
pub struct Cli {
    /// Datadog site suffix or full API base URL.
    /// Examples: datadoghq.com, us3.datadoghq.com, https://api.datadoghq.com
    #[arg(long)]
    pub site: Option<String>,

    /// Datadog API key (falls back to DD_API_KEY)
    #[arg(long)]
    pub api_key: Option<String>,

    /// Datadog application key (falls back to DD_APP_KEY or DD_APPLICATION_KEY)
    #[arg(long)]
    pub app_key: Option<String>,

    /// Print compact JSON
    #[arg(long)]
    pub compact: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Query logs via /api/v2/logs/events/search
    Logs {
        /// Datadog log query string
        #[arg(value_name = "QUERY")]
        query: String,
        /// Start time; supports RFC3339 or relative expressions like now-15m
        #[arg(long, default_value = "now-15m")]
        from: String,
        /// End time; supports RFC3339 or relative expressions like now
        #[arg(long, default_value = "now")]
        to: String,
        /// Result count (max currently enforced by Datadog API)
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// Sort order: asc or desc
        #[arg(long, default_value = "desc")]
        sort: String,
        /// Pagination cursor from previous response
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Query metrics via /api/v1/query
    Metrics {
        /// Datadog metric query expression
        #[arg(value_name = "QUERY")]
        query: String,
        /// Start time; supports unix seconds, RFC3339, now-15m, now-1h, now-2d
        #[arg(long, default_value = "now-15m")]
        from: String,
        /// End time; supports unix seconds, RFC3339, now
        #[arg(long, default_value = "now")]
        to: String,
    },
    /// Query events via /api/v2/events
    Events {
        /// Optional Datadog event query string
        #[arg(long)]
        query: Option<String>,
        /// Start time; supports RFC3339 or relative expressions like now-15m
        #[arg(long, default_value = "now-15m")]
        from: String,
        /// End time; supports RFC3339 or relative expressions like now
        #[arg(long, default_value = "now")]
        to: String,
        /// Result count
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// Sort order: asc or desc
        #[arg(long, default_value = "desc")]
        sort: String,
    },
    /// Generic Datadog API call for unsupported endpoints
    Raw {
        /// HTTP method (GET, POST, PUT, DELETE)
        #[arg(long)]
        method: String,
        /// Path beginning with /api/... or full URL
        #[arg(long)]
        path: String,
        /// Query parameters as repeated key=value
        #[arg(long = "query")]
        query_params: Vec<String>,
        /// Raw JSON body string
        #[arg(long)]
        body: Option<String>,
        /// Read JSON body from file
        #[arg(long)]
        body_file: Option<PathBuf>,
    },
}
