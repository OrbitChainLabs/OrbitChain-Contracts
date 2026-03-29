//! Horizon API Client Errors
//!
//! Comprehensive error types for Horizon client operations.

use std::time::Duration;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during Horizon API interactions
#[derive(Error, Debug)]
pub enum HorizonError {
    /// Network connectivity error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// HTTP request failed
    #[error("HTTP request failed with status {status}: {message}")]
    HttpError { status: u16, message: String },

    /// Request timeout
    #[error("Request timeout after {duration:?}")]
    Timeout { duration: Duration },

    /// Rate limit exceeded
    #[error("Rate limit exceeded. Retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },

    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Invalid response format
    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    /// Server error (5xx)
    #[error("Server error ({status}): {message}")]
    ServerError { status: u16, message: String },

    /// Not found error (404)
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Bad request error (400)
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Unauthorized error (401)
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden error (403)
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Connection refused
    #[error("Connection refused: {0}")]
    ConnectionRefused(String),

    /// Connection reset
    #[error("Connection reset: {0}")]
    ConnectionReset(String),

    /// DNS resolution failed
    #[error("DNS resolution failed: {0}")]
    DnsError(String),

    /// TLS error
    #[error("TLS error: {0}")]
    TlsError(String),

    /// Horizon service unavailable
    #[error("Horizon service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Cache error
    #[error("Cache error: {0}")]
    CacheError(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// URL parsing error
    #[error("URL parsing error: {0}")]
    UrlError(#[from] url::ParseError),

    /// Other errors
    #[error("Error: {0}")]
    Other(String),
}

/// Horizon error severity levels for prioritizing responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Critical errors requiring immediate attention
    Critical,
    /// High severity errors that block operations
    High,
    /// Medium severity transient failures
    Medium,
    /// Low severity informational errors
    Low,
}

impl HorizonError {
    /// Get the severity level of this error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            // Critical - security or data integrity issues
            HorizonError::TlsError(_) | HorizonError::InvalidResponse(_) => ErrorSeverity::Critical,
            
            // High - service unavailable
            HorizonError::ServiceUnavailable(_) 
            | HorizonError::ServerError { status: 503, .. } => ErrorSeverity::High,
            
            // Medium - transient failures
            HorizonError::NetworkError(_)
            | HorizonError::Timeout { .. }
            | HorizonError::ConnectionRefused(_)
            | HorizonError::ConnectionReset(_)
            | HorizonError::DnsError(_)
            | HorizonError::RateLimited { .. }
            | HorizonError::ServerError { .. } => ErrorSeverity::Medium,
            
            // Low - client errors or expected failures
            HorizonError::InvalidRequest(_)
            | HorizonError::BadRequest(_)
            | HorizonError::Unauthorized(_)
            | HorizonError::Forbidden(_)
            | HorizonError::NotFound(_)
            | HorizonError::CacheError(_)
            | HorizonError::InvalidConfig(_)
            | HorizonError::Other(_) => ErrorSeverity::Low,
            
            // Default to medium for HTTP errors
            HorizonError::HttpError { status, .. } if *status >= 500 => ErrorSeverity::Medium,
            HorizonError::HttpError { .. } => ErrorSeverity::Low,
            
            // URL/JSON parsing errors are low severity
            HorizonError::UrlError(_) | HorizonError::JsonError(_) => ErrorSeverity::Low,
        }
    }

    /// Categorize error for better handling
    pub fn category(&self) -> &'static str {
        match self {
            HorizonError::NetworkError(_) 
            | HorizonError::ConnectionRefused(_)
            | HorizonError::ConnectionReset(_)
            | HorizonError::DnsError(_) => "network",
            
            HorizonError::Timeout { .. } => "timeout",
            
            HorizonError::RateLimited { .. } => "rate_limit",
            
            HorizonError::HttpError { .. }
            | HorizonError::BadRequest(_)
            | HorizonError::Unauthorized(_)
            | HorizonError::Forbidden(_)
            | HorizonError::NotFound(_) => "http_client",
            
            HorizonError::ServerError { .. }
            | HorizonError::ServiceUnavailable(_) => "server",
            
            HorizonError::InvalidRequest(_) => "validation",
            
            HorizonError::InvalidResponse(_) => "response",
            
            HorizonError::TlsError(_) => "security",
            
            HorizonError::CacheError(_) => "cache",
            
            HorizonError::InvalidConfig(_) => "configuration",
            
            HorizonError::UrlError(_) => "url",
            
            HorizonError::JsonError(_) => "parsing",
            
            HorizonError::Other(_) => "other",
        }
    }

    /// Canonical code for the error (machine-readable)
    pub fn error_code(&self) -> &'static str {
        match self {
            HorizonError::NetworkError(_) => "network_error",
            HorizonError::ConnectionRefused(_) => "connection_refused",
            HorizonError::ConnectionReset(_) => "connection_reset",
            HorizonError::DnsError(_) => "dns_error",
            HorizonError::Timeout { .. } => "timeout",
            HorizonError::RateLimited { .. } => "rate_limited",
            HorizonError::HttpError { .. } => "http_error",
            HorizonError::NotFound(_) => "not_found",
            HorizonError::BadRequest(_) => "bad_request",
            HorizonError::Unauthorized(_) => "unauthorized",
            HorizonError::Forbidden(_) => "forbidden",
            HorizonError::InvalidRequest(_) => "invalid_request",
            HorizonError::InvalidResponse(_) => "invalid_response",
            HorizonError::ServerError { .. } => "server_error",
            HorizonError::ServiceUnavailable(_) => "service_unavailable",
            HorizonError::TlsError(_) => "tls_error",
            HorizonError::CacheError(_) => "cache_error",
            HorizonError::InvalidConfig(_) => "invalid_config",
            HorizonError::UrlError(_) => "url_parse_error",
            HorizonError::JsonError(_) => "json_parse_error",
            HorizonError::Other(_) => "other",
        }
    }

    /// Suggested retry delay value in seconds
    pub fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            HorizonError::RateLimited { retry_after } => Some(retry_after.as_secs()),
            OutlookError::ServerError { .. } => Some(5),
            HorizonError::ServiceUnavailable(_) => Some(10),
            HorizonError::Timeout { duration } => Some(duration.as_secs()),
            _ => None,
        }
    }

    /// Standardized error response structure for API consumption.
    pub fn to_response(&self) -> HorizonErrorResponse {
        HorizonErrorResponse {
            code: self.error_code().to_string(),
            category: self.category().to_string(),
            severity: format!("{:?}", self.severity()),
            message: self.to_string(),
            details: Some(self.error_context()),
            retry_after_seconds: self.retry_after_seconds(),
        }
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            HorizonError::NetworkError(_)
                | HorizonError::Timeout { .. }
                | HorizonError::RateLimited { .. }
                | HorizonError::ConnectionRefused(_)
                | HorizonError::ConnectionReset(_)
                | HorizonError::ServerError { .. }
                | HorizonError::ServiceUnavailable(_)
                | HorizonError::DnsError(_)
        )
    }

    /// Check if this is a rate limit error
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, HorizonError::RateLimited { .. })
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        matches!(
            self,
            HorizonError::ServerError { .. } | HorizonError::ServiceUnavailable(_)
        )
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            HorizonError::InvalidRequest(_)
                | HorizonError::BadRequest(_)
                | HorizonError::Unauthorized(_)
                | HorizonError::Forbidden(_)
                | HorizonError::NotFound(_)
        )
    }

    /// Get detailed context about the error for logging
    pub fn error_context(&self) -> String {
        match self {
            HorizonError::RateLimited { retry_after } => {
                format!("Rate limited by Horizon API, retry after {}s", retry_after.as_secs())
            },
            HorizonError::Timeout { duration } => {
                format!("Request timed out after {}s", duration.as_secs())
            },
            HorizonError::ServerError { status, message } => {
                format!("Horizon server error ({}): {}", status, message)
            },
            HorizonError::NetworkError(msg) => {
                format!("Network connectivity issue: {}", msg)
            },
            HorizonError::ConnectionRefused(msg) => {
                format!("Connection refused by Horizon: {}", msg)
            },
            HorizonError::DnsError(msg) => {
                format!("DNS resolution failed: {}", msg)
            },
            _ => self.to_string(),
        }
    }
    pub fn suggested_retry_duration(&self) -> Option<Duration> {
        match self {
            HorizonError::RateLimited { retry_after } => Some(*retry_after),
            HorizonError::ServerError { .. } => Some(Duration::from_secs(5)),
            HorizonError::ServiceUnavailable(_) => Some(Duration::from_secs(10)),
            HorizonError::Timeout { .. } => Some(Duration::from_secs(2)),
            _ => None,
        }
    }

    /// Convert reqwest error to HorizonError
    pub fn from_reqwest(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            HorizonError::Timeout {
                duration: Duration::from_secs(30),
            }
        } else if err.is_connect() {
            HorizonError::ConnectionRefused(err.to_string())
        } else if err.is_request() {
            HorizonError::NetworkError(err.to_string())
        } else if let Some(status) = err.status() {
            match status {
                StatusCode::NOT_FOUND => HorizonError::NotFound(err.to_string()),
                StatusCode::BAD_REQUEST => HorizonError::BadRequest(err.to_string()),
                StatusCode::UNAUTHORIZED => HorizonError::Unauthorized(err.to_string()),
                StatusCode::FORBIDDEN => HorizonError::Forbidden(err.to_string()),
                _ if status.is_server_error() => HorizonError::ServerError {
                    status: status.as_u16(),
                    message: err.to_string(),
                },
                _ => HorizonError::HttpError {
                    status: status.as_u16(),
                    message: err.to_string(),
                },
            }
        } else {
            HorizonError::NetworkError(err.to_string())
        }
    }
}

/// Result type for Horizon operations
pub type HorizonResult<T> = Result<T, HorizonError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable() {
        let network_err = HorizonError::NetworkError("connection failed".to_string());
        assert!(network_err.is_retryable());

        let not_found = HorizonError::NotFound("resource not found".to_string());
        assert!(!not_found.is_retryable());
    }

    #[test]
    fn test_is_server_error() {
        let server_err = HorizonError::ServerError {
            status: 500,
            message: "internal error".to_string(),
        };
        assert!(server_err.is_server_error());

        let client_err = HorizonError::BadRequest("invalid".to_string());
        assert!(!client_err.is_server_error());
    }

    #[test]
    fn test_is_client_error() {
        let bad_request = HorizonError::BadRequest("invalid".to_string());
        assert!(bad_request.is_client_error());

        let server_err = HorizonError::ServerError {
            status: 500,
            message: "error".to_string(),
        };
        assert!(!server_err.is_client_error());
    }

    #[test]
    fn test_suggested_retry_duration() {
        let rate_limited = HorizonError::RateLimited {
            retry_after: Duration::from_secs(60),
        };
        assert_eq!(
            rate_limited.suggested_retry_duration(),
            Some(Duration::from_secs(60))
        );

        let not_found = HorizonError::NotFound("not found".to_string());
        assert_eq!(not_found.suggested_retry_duration(), None);
    }
}
