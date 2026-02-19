use serde_json::{Value, json};

use crate::datadog::DatadogError;

#[derive(Debug)]
pub enum AppError {
    Usage(String),
    Auth {
        status: u16,
        message: String,
    },
    RateLimited {
        message: String,
        retry_after_ms: Option<u64>,
    },
    Upstream {
        status: Option<u16>,
        message: String,
    },
    Api {
        status: u16,
        message: String,
    },
    Internal(String),
}

impl AppError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Auth { .. } => 3,
            Self::RateLimited { .. } => 4,
            Self::Upstream { .. } => 5,
            Self::Api { .. } => 6,
            Self::Internal(_) => 1,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Usage(message) => json!({
                "error": {
                    "category": "usage",
                    "exit_code": self.exit_code(),
                    "retryable": false,
                    "message": message,
                }
            }),
            Self::Auth { status, message } => json!({
                "error": {
                    "category": "auth",
                    "exit_code": self.exit_code(),
                    "status": status,
                    "retryable": false,
                    "message": message,
                }
            }),
            Self::RateLimited {
                message,
                retry_after_ms,
            } => json!({
                "error": {
                    "category": "rate_limit",
                    "exit_code": self.exit_code(),
                    "status": 429,
                    "retryable": false,
                    "retry_after_ms": retry_after_ms,
                    "message": message,
                }
            }),
            Self::Upstream { status, message } => json!({
                "error": {
                    "category": "upstream",
                    "exit_code": self.exit_code(),
                    "status": status,
                    "retryable": true,
                    "message": message,
                }
            }),
            Self::Api { status, message } => json!({
                "error": {
                    "category": "api",
                    "exit_code": self.exit_code(),
                    "status": status,
                    "retryable": false,
                    "message": message,
                }
            }),
            Self::Internal(message) => json!({
                "error": {
                    "category": "internal",
                    "exit_code": self.exit_code(),
                    "retryable": false,
                    "message": message,
                }
            }),
        }
    }
}

impl From<DatadogError> for AppError {
    fn from(value: DatadogError) -> Self {
        match value {
            DatadogError::InvalidRequest(message) => Self::Usage(message),
            DatadogError::Auth { status, body } => Self::Auth {
                status,
                message: body,
            },
            DatadogError::RateLimited {
                body,
                retry_after_ms,
            } => Self::RateLimited {
                message: body,
                retry_after_ms,
            },
            DatadogError::Retryable { status, message } => Self::Upstream { status, message },
            DatadogError::Api { status, body } => Self::Api {
                status,
                message: body,
            },
        }
    }
}
