use serde_json::{Value, json};

use crate::datadog::{DatadogError, RateLimitInfo};

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
        retried: u32,
        rate_limit_info: Option<RateLimitInfo>,
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
                retried,
                rate_limit_info,
            } => {
                let mut error = json!({
                    "category": "rate_limit",
                    "exit_code": self.exit_code(),
                    "status": 429,
                    "retryable": true,
                    "retried": retried,
                    "retry_after_ms": retry_after_ms,
                    "message": message,
                });
                if let Some(info) = rate_limit_info
                    && let Some(obj) = error.as_object_mut()
                {
                    if let Some(remaining) = info.remaining {
                        obj.insert("rate_limit_remaining".to_string(), json!(remaining));
                    }
                    if let Some(limit) = info.limit {
                        obj.insert("rate_limit_limit".to_string(), json!(limit));
                    }
                    if let Some(reset) = info.reset {
                        obj.insert("rate_limit_reset".to_string(), json!(reset));
                    }
                    if let Some(period) = info.period {
                        obj.insert("rate_limit_period".to_string(), json!(period));
                    }
                }
                json!({ "error": error })
            }
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
                retried,
                rate_limit_info,
            } => Self::RateLimited {
                message: body,
                retry_after_ms,
                retried,
                rate_limit_info,
            },
            DatadogError::Retryable { status, message } => Self::Upstream { status, message },
            DatadogError::Api { status, body } => Self::Api {
                status,
                message: body,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limited_error_json_retryable_true() {
        let err = AppError::RateLimited {
            message: "too many requests".to_string(),
            retry_after_ms: Some(5000),
            retried: 3,
            rate_limit_info: Some(RateLimitInfo {
                remaining: Some(0),
                limit: Some(300),
                reset: Some(1700000000),
                period: Some(60),
            }),
        };
        let json = err.to_json();
        let error = &json["error"];
        assert_eq!(error["category"], "rate_limit");
        assert_eq!(error["status"], 429);
        assert_eq!(error["retryable"], true);
        assert_eq!(error["retried"], 3);
        assert_eq!(error["retry_after_ms"], 5000);
        assert_eq!(error["rate_limit_remaining"], 0);
        assert_eq!(error["rate_limit_limit"], 300);
        assert_eq!(error["rate_limit_reset"], 1700000000);
        assert_eq!(error["rate_limit_period"], 60);
        assert_eq!(error["message"], "too many requests");
    }

    #[test]
    fn test_rate_limited_error_json_without_rate_limit_info() {
        let err = AppError::RateLimited {
            message: "rate limited".to_string(),
            retry_after_ms: None,
            retried: 0,
            rate_limit_info: None,
        };
        let json = err.to_json();
        let error = &json["error"];
        assert_eq!(error["retryable"], true);
        assert_eq!(error["retried"], 0);
        assert_eq!(error["retry_after_ms"], Value::Null);
        assert!(error.get("rate_limit_remaining").is_none());
    }

    #[test]
    fn test_rate_limited_error_exit_code() {
        let err = AppError::RateLimited {
            message: "rate limited".to_string(),
            retry_after_ms: None,
            retried: 0,
            rate_limit_info: None,
        };
        assert_eq!(err.exit_code(), 4);
    }
}

