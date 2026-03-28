//! Transaction Submission Errors
//!
//! Comprehensive error types for transaction submission with categorization
//! for different failure scenarios.

use std::time::Duration;
use thiserror::Error;

/// Errors that can occur during transaction submission
#[derive(Error, Debug, Clone)]
pub enum SubmissionError {
    /// Network connectivity error
    #[error("Network error: {message}")]
    NetworkError {
        message: String,
        is_retryable: bool,
    },

    /// Request timeout
    #[error("Submission timeout after {duration:?}")]
    Timeout {
        duration: Duration,
        attempts: u32,
    },

    /// Insufficient balance for transaction
    #[error("Insufficient balance: {message}")]
    InsufficientBalance {
        message: String,
        account: String,
        required: Option<String>,
        available: Option<String>,
    },

    /// Invalid sequence number
    #[error("Invalid sequence number: {message}")]
    InvalidSequence {
        message: String,
        current_sequence: Option<u64>,
        expected_sequence: Option<u64>,
    },

    /// Transaction failed with a bad result
    #[error("Transaction failed: {result_code} - {message}")]
    TransactionFailed {
        result_code: String,
        message: String,
        operation_results: Vec<OperationFailure>,
    },

    /// Transaction was rejected before submission
    #[error("Transaction rejected: {message}")]
    TransactionRejected {
        message: String,
        reason_code: String,
    },

    /// Duplicate transaction submission
    #[error("Duplicate transaction: {message}")]
    DuplicateTransaction {
        message: String,
        transaction_hash: String,
    },

    /// Invalid transaction envelope
    #[error("Invalid transaction envelope: {message}")]
    InvalidEnvelope {
        message: String,
    },

    /// Horizon server error
    #[error("Horizon server error ({status}): {message}")]
    ServerError {
        status: u16,
        message: String,
    },

    /// Rate limit exceeded
    #[error("Rate limit exceeded. Retry after {retry_after:?}")]
    RateLimited {
        retry_after: Duration,
    },

    /// Transaction too large
    #[error("Transaction too large: {size} bytes (max {max_size})")]
    TransactionTooLarge {
        size: usize,
        max_size: usize,
    },

    /// Fee too low
    #[error("Fee too low: {fee} stroops (minimum {min_fee})")]
    FeeTooLow {
        fee: u64,
        min_fee: u64,
    },

    /// Operation not supported
    #[error("Operation not supported: {message}")]
    OperationNotSupported {
        message: String,
    },

    /// Internal error
    #[error("Internal error: {message}")]
    InternalError {
        message: String,
    },

    /// Submission was cancelled
    #[error("Submission cancelled: {reason}")]
    Cancelled {
        reason: String,
    },

    /// Maximum retry attempts exceeded
    #[error("Max retry attempts ({attempts}) exceeded: {last_error}")]
    MaxRetriesExceeded {
        attempts: u32,
        last_error: String,
    },

    /// Unknown error
    #[error("Unknown error: {message}")]
    Unknown {
        message: String,
    },

    /// Invalid response format
    #[error("Invalid response: {message}")]
    InvalidResponse {
        message: String,
    },
}

/// Operation failure details
#[derive(Debug, Clone)]
pub struct OperationFailure {
    /// Operation index
    pub index: u32,
    /// Result code
    pub result_code: String,
    /// Human-readable description
    pub description: String,
}

impl std::fmt::Display for OperationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Op[{}]: {} - {}", self.index, self.result_code, self.description)
    }
}

/// Result type for submission operations
pub type SubmissionResult<T> = Result<T, SubmissionError>;

impl SubmissionError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            SubmissionError::NetworkError { is_retryable: true, .. }
                | SubmissionError::Timeout { .. }
                | SubmissionError::InvalidSequence { .. }
                | SubmissionError::RateLimited { .. }
                | SubmissionError::ServerError { .. }
        )
    }

    /// Check if this error indicates the transaction was already submitted
    pub fn is_duplicate(&self) -> bool {
        matches!(self, SubmissionError::DuplicateTransaction { .. })
    }

    /// Check if this is a sequence number error (can retry with updated sequence)
    pub fn is_sequence_error(&self) -> bool {
        matches!(self, SubmissionError::InvalidSequence { .. })
    }

    /// Check if this is a balance error (don't retry - won't succeed)
    pub fn is_balance_error(&self) -> bool {
        matches!(self, SubmissionError::InsufficientBalance { .. })
    }

    /// Check if this is a fee error (might retry with higher fee)
    pub fn is_fee_error(&self) -> bool {
        matches!(self, SubmissionError::FeeTooLow { .. })
    }

    /// Get the error code for categorization
    pub fn error_code(&self) -> &'static str {
        match self {
            SubmissionError::NetworkError { .. } => "network_error",
            SubmissionError::Timeout { .. } => "tx_timeout",
            SubmissionError::InsufficientBalance { .. } => "tx_insufficient_balance",
            SubmissionError::InvalidSequence { .. } => "tx_bad_seq",
            SubmissionError::TransactionFailed { result_code, .. } => result_code.as_str(),
            SubmissionError::TransactionRejected { reason_code, .. } => reason_code.as_str(),
            SubmissionError::DuplicateTransaction { .. } => "tx_duplicate",
            SubmissionError::InvalidEnvelope { .. } => "tx_invalid_envelope",
            SubmissionError::ServerError { .. } => "server_error",
            SubmissionError::RateLimited { .. } => "rate_limited",
            SubmissionError::TransactionTooLarge { .. } => "tx_too_large",
            SubmissionError::FeeTooLow { .. } => "tx_fee_too_low",
            SubmissionError::OperationNotSupported { .. } => "op_not_supported",
            SubmissionError::InternalError { .. } => "internal_error",
            SubmissionError::Cancelled { .. } => "cancelled",
            SubmissionError::MaxRetriesExceeded { .. } => "max_retries_exceeded",
            SubmissionError::Unknown { .. } => "unknown",
        }
    }

    /// Get a human-readable error message
    pub fn user_message(&self) -> String {
        match self {
            SubmissionError::InsufficientBalance { message, .. } => {
                format!("Insufficient balance: {}", message)
            }
            SubmissionError::InvalidSequence { message, .. } => {
                format!("Sequence number mismatch: {}. Please try again.", message)
            }
            SubmissionError::Timeout { .. } => {
                "Transaction submission timed out. Please check your transaction status.".to_string()
            }
            SubmissionError::DuplicateTransaction { .. } => {
                "This transaction has already been submitted.".to_string()
            }
            SubmissionError::FeeTooLow { fee, min_fee } => {
                format!("Fee too low: {} stroops. Minimum required: {} stroops.", fee, min_fee)
            }
            _ => self.to_string(),
        }
    }

    /// Get suggested retry duration if available
    pub fn suggested_retry_duration(&self) -> Option<Duration> {
        match self {
            SubmissionError::RateLimited { retry_after } => Some(*retry_after),
            SubmissionError::InvalidSequence { .. } => Some(Duration::from_millis(100)),
            SubmissionError::ServerError { .. } => Some(Duration::from_secs(2)),
            SubmissionError::Timeout { .. } => Some(Duration::from_secs(1)),
            SubmissionError::NetworkError { .. } => Some(Duration::from_millis(500)),
            _ => None,
        }
    }

    /// Convert from Horizon error response JSON
    pub fn from_horizon_response(status: u16, response_body: &str) -> Self {
        // Try to parse the response as JSON
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(response_body) {
            // Extract extras if available
            if let Some(extras) = json.get("extras") {
                // Check for result codes
                if let Some(result_codes) = extras.get("result_codes") {
                    let transaction_code = result_codes
                        .get("transaction")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let operation_codes: Vec<String> = result_codes
                        .get("operations")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    return Self::from_result_code(
                        transaction_code,
                        &operation_codes,
                        response_body,
                    );
                }

                // Check for envelope XDR error
                if let Some(envelope_xdr) = extras.get("envelope_xdr") {
                    if envelope_xdr.is_null() || envelope_xdr.as_str() == Some("") {
                        return SubmissionError::InvalidEnvelope {
                            message: "Invalid transaction envelope".to_string(),
                        };
                    }
                }
            }

            // Check for title/detail
            if let Some(title) = json.get("title").and_then(|v| v.as_str()) {
                if title == "Transaction Failed" {
                    return SubmissionError::TransactionFailed {
                        result_code: "tx_failed".to_string(),
                        message: json
                            .get("detail")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Transaction failed")
                            .to_string(),
                        operation_results: vec![],
                    };
                }
            }
        }

        // Fallback to status-based error
        match status {
            400 => SubmissionError::TransactionRejected {
                message: response_body.to_string(),
                reason_code: "tx_malformed".to_string(),
            },
            404 => SubmissionError::Unknown {
                message: "Transaction or endpoint not found (HTTP 404)".to_string(),
            },
            429 => SubmissionError::RateLimited {
                retry_after: Duration::from_secs(60),
            },
            500..=599 => SubmissionError::ServerError {
                status,
                message: response_body.to_string(),
            },
            _ => SubmissionError::Unknown {
                message: format!("HTTP {}: {}", status, response_body),
            },
        }
    }

    /// Create error from result code
    fn from_result_code(
        code: &str,
        operation_codes: &[String],
        raw_response: &str,
    ) -> SubmissionError {
        let op_failures: Vec<OperationFailure> = operation_codes
            .iter()
            .enumerate()
            .map(|(idx, code)| OperationFailure {
                index: idx as u32,
                result_code: code.clone(),
                description: Self::describe_operation_result(code),
            })
            .collect();

        match code {
            "tx_insufficient_balance" => SubmissionError::InsufficientBalance {
                message: "Source account has insufficient balance".to_string(),
                account: String::new(),
                required: None,
                available: None,
            },
            "tx_bad_seq" => SubmissionError::InvalidSequence {
                message: "Sequence number does not match source account".to_string(),
                current_sequence: None,
                expected_sequence: None,
            },
            "tx_insufficient_fee" => SubmissionError::FeeTooLow {
                fee: 0,
                min_fee: 0,
            },
            "tx_bad_auth" => SubmissionError::TransactionRejected {
                message: "Transaction has invalid or missing signatures".to_string(),
                reason_code: "tx_bad_auth".to_string(),
            },
            "tx_bad_auth_extra" => SubmissionError::TransactionRejected {
                message: "Transaction has extra signatures".to_string(),
                reason_code: "tx_bad_auth_extra".to_string(),
            },
            "tx_no_source_account" => SubmissionError::TransactionRejected {
                message: "Source account does not exist".to_string(),
                reason_code: "tx_no_source_account".to_string(),
            },
            "tx_too_early" => SubmissionError::TransactionRejected {
                message: "Transaction time bounds are too early".to_string(),
                reason_code: "tx_too_early".to_string(),
            },
            "tx_too_late" => SubmissionError::TransactionRejected {
                message: "Transaction time bounds have expired".to_string(),
                reason_code: "tx_too_late".to_string(),
            },
            "tx_missing_operation" => SubmissionError::TransactionRejected {
                message: "Transaction has no operations".to_string(),
                reason_code: "tx_missing_operation".to_string(),
            },
            "tx_bad_min_seq_age_or_gap" => SubmissionError::TransactionRejected {
                message: "Invalid minimum sequence age or gap".to_string(),
                reason_code: "tx_bad_min_seq_age_or_gap".to_string(),
            },
            "tx_malformed" => SubmissionError::InvalidEnvelope {
                message: "Transaction envelope is malformed".to_string(),
            },
            "tx_failed" | _ => SubmissionError::TransactionFailed {
                result_code: code.to_string(),
                message: format!("Transaction failed: {}", code),
                operation_results: op_failures,
            },
        }
    }

    /// Get description for operation result code
    fn describe_operation_result(code: &str) -> String {
        match code {
            "op_success" => "Operation successful".to_string(),
            "op_malformed" => "Operation malformed".to_string(),
            "op_underfunded" => "Not enough funds for operation".to_string(),
            "op_low_reserve" => "Would create an account below the minimum reserve".to_string(),
            "op_line_full" => "Trust line would exceed limit".to_string(),
            "op_no_issuer" => "Asset issuer does not exist".to_string(),
            "op_no_trust" => "Trust line not found".to_string(),
            "op_not_authorized" => "Not authorized to hold asset".to_string(),
            "op_src_no_trust" => "Source trust line not found".to_string(),
            "op_src_not_authorized" => "Source not authorized".to_string(),
            "op_no_destination" => "Destination account not found".to_string(),
            "op_already_exists" => "Account already exists".to_string(),
            "op_invalid_limit" => "Invalid limit for trust line".to_string(),
            "op_bad_auth" => "Invalid authorization".to_string(),
            _ => format!("Unknown operation result: {}", code),
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        let network_err = SubmissionError::NetworkError {
            message: "connection failed".to_string(),
            is_retryable: true,
        };
        assert!(network_err.is_retryable());

        let balance_err = SubmissionError::InsufficientBalance {
            message: "no funds".to_string(),
            account: "G...".to_string(),
            required: None,
            available: None,
        };
        assert!(!balance_err.is_retryable());
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            SubmissionError::InsufficientBalance {
                message: "".to_string(),
                account: "".to_string(),
                required: None,
                available: None,
            }
            .error_code(),
            "tx_insufficient_balance"
        );

        assert_eq!(
            SubmissionError::InvalidSequence {
                message: "".to_string(),
                current_sequence: None,
                expected_sequence: None,
            }
            .error_code(),
            "tx_bad_seq"
        );

        assert_eq!(
            SubmissionError::Timeout {
                duration: Duration::from_secs(30),
                attempts: 1,
            }
            .error_code(),
            "tx_timeout"
        );
    }

    #[test]
    fn test_from_horizon_response_insufficient_balance() {
        let response = r#"{
            "type": "https://stellar.org/horizon-errors/transaction_failed",
            "title": "Transaction Failed",
            "status": 400,
            "extras": {
                "result_codes": {
                    "transaction": "tx_insufficient_balance"
                }
            }
        }"#;

        let error = SubmissionError::from_horizon_response(400, response);
        assert!(matches!(error, SubmissionError::InsufficientBalance { .. }));
        assert_eq!(error.error_code(), "tx_insufficient_balance");
    }

    #[test]
    fn test_from_horizon_response_bad_seq() {
        let response = r#"{
            "type": "https://stellar.org/horizon-errors/transaction_failed",
            "title": "Transaction Failed",
            "status": 400,
            "extras": {
                "result_codes": {
                    "transaction": "tx_bad_seq"
                }
            }
        }"#;

        let error = SubmissionError::from_horizon_response(400, response);
        assert!(matches!(error, SubmissionError::InvalidSequence { .. }));
        assert!(error.is_sequence_error());
    }

    #[test]
    fn test_user_messages() {
        let timeout = SubmissionError::Timeout {
            duration: Duration::from_secs(30),
            attempts: 1,
        };
        assert!(timeout.user_message().contains("timed out"));

        let duplicate = SubmissionError::DuplicateTransaction {
            message: "already submitted".to_string(),
            transaction_hash: "abc".to_string(),
        };
        assert!(duplicate.user_message().contains("already been submitted"));
    }
}
