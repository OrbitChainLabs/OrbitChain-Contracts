//! Transaction Verification Errors
//!
//! Error types for on-chain transaction verification with clear categorization
//! for each failure scenario encountered when querying Horizon.

use std::time::Duration;
use thiserror::Error;

/// Errors that can occur during transaction verification
#[derive(Error, Debug, Clone)]
pub enum VerificationError {
    /// Transaction hash not found on-chain
    #[error("Transaction not found on-chain: {hash}")]
    NotFound { hash: String },

    /// Network connectivity error
    #[error("Network error: {message}")]
    NetworkError { message: String },

    /// Request timed out before receiving a response
    #[error("Verification request timed out after {duration:?}")]
    Timeout { duration: Duration },

    /// Horizon rate limit exceeded
    #[error("Rate limit exceeded. Retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },

    /// Horizon returned an unparseable or unexpected response
    #[error("Invalid Horizon response: {message}")]
    InvalidResponse { message: String },

    /// Horizon server-side error
    #[error("Horizon server error (HTTP {status}): {message}")]
    ServerError { status: u16, message: String },

    /// The supplied transaction hash is not a valid 64-character hex string
    #[error("Invalid transaction hash format: '{hash}'")]
    InvalidHash { hash: String },

    /// Internal service error (e.g. HTTP client construction)
    #[error("Internal error: {message}")]
    InternalError { message: String },
}

impl VerificationError {
    /// Whether this error is transient and worth retrying
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            VerificationError::NetworkError { .. }
                | VerificationError::Timeout { .. }
                | VerificationError::RateLimited { .. }
                | VerificationError::ServerError { .. }
        )
    }

    /// Short machine-readable error code
    pub fn error_code(&self) -> &'static str {
        match self {
            VerificationError::NotFound { .. } => "not_found",
            VerificationError::NetworkError { .. } => "network_error",
            VerificationError::Timeout { .. } => "timeout",
            VerificationError::RateLimited { .. } => "rate_limited",
            VerificationError::InvalidResponse { .. } => "invalid_response",
            VerificationError::ServerError { .. } => "server_error",
            VerificationError::InvalidHash { .. } => "invalid_hash",
            VerificationError::InternalError { .. } => "internal_error",
        }
    }

    /// Suggested wait time before retrying, if applicable
    pub fn suggested_retry_duration(&self) -> Option<Duration> {
        match self {
            VerificationError::RateLimited { retry_after } => Some(*retry_after),
            VerificationError::Timeout { .. } => Some(Duration::from_secs(2)),
            VerificationError::NetworkError { .. } => Some(Duration::from_millis(500)),
            VerificationError::ServerError { .. } => Some(Duration::from_secs(3)),
            _ => None,
        }
    }
}

/// Result type for verification operations
pub type VerificationResult<T> = Result<T, VerificationError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(VerificationError::NetworkError {
            message: "connection refused".into()
        }
        .is_retryable());

        assert!(VerificationError::Timeout {
            duration: Duration::from_secs(30)
        }
        .is_retryable());

        assert!(VerificationError::RateLimited {
            retry_after: Duration::from_secs(60)
        }
        .is_retryable());

        assert!(VerificationError::ServerError {
            status: 503,
            message: "unavailable".into()
        }
        .is_retryable());
    }

    #[test]
    fn test_non_retryable_errors() {
        assert!(!VerificationError::NotFound {
            hash: "abc".into()
        }
        .is_retryable());

        assert!(!VerificationError::InvalidHash {
            hash: "bad".into()
        }
        .is_retryable());

        assert!(!VerificationError::InvalidResponse {
            message: "parse failed".into()
        }
        .is_retryable());
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            VerificationError::NotFound { hash: "x".into() }.error_code(),
            "not_found"
        );
        assert_eq!(
            VerificationError::InvalidHash { hash: "x".into() }.error_code(),
            "invalid_hash"
        );
        assert_eq!(
            VerificationError::Timeout {
                duration: Duration::from_secs(10)
            }
            .error_code(),
            "timeout"
        );
    }

    #[test]
    fn test_suggested_retry_duration() {
        let rate_limited = VerificationError::RateLimited {
            retry_after: Duration::from_secs(42),
        };
        assert_eq!(
            rate_limited.suggested_retry_duration(),
            Some(Duration::from_secs(42))
        );

        let not_found = VerificationError::NotFound { hash: "x".into() };
        assert!(not_found.suggested_retry_duration().is_none());
    }
}
