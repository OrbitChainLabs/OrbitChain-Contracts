//! Transaction Verification Types
//!
//! Core request/response types for on-chain transaction verification via Horizon.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// Outcome of an on-chain verification check
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Transaction is on-chain and marked successful
    Confirmed,
    /// Transaction is on-chain but the ledger marked it as failed
    Failed,
    /// Transaction hash was not found on Horizon
    NotFound,
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationStatus::Confirmed => write!(f, "confirmed"),
            VerificationStatus::Failed => write!(f, "failed"),
            VerificationStatus::NotFound => write!(f, "not_found"),
        }
    }
}

/// Request to verify a transaction by its hash
#[derive(Debug, Clone)]
pub struct VerificationRequest {
    /// 64-character hex transaction hash
    pub transaction_hash: String,
    /// How long to wait for the Horizon response
    pub timeout: Duration,
}

impl VerificationRequest {
    /// Create a new verification request with a 30-second default timeout
    pub fn new(transaction_hash: impl Into<String>) -> Self {
        Self {
            transaction_hash: transaction_hash.into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Override the timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Result of verifying a transaction on-chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResponse {
    /// The transaction hash that was verified
    pub transaction_hash: String,
    /// High-level verification outcome
    pub status: VerificationStatus,
    /// Raw `successful` field from Horizon (`None` if transaction was not found)
    pub successful: Option<bool>,
    /// Ledger sequence the transaction was included in
    pub ledger_sequence: Option<u32>,
    /// ISO-8601 close time of that ledger as returned by Horizon
    pub ledger_close_time: Option<String>,
    /// Fee charged in stroops (string to preserve precision)
    pub fee_charged: Option<String>,
    /// Transaction-level result code (e.g. `"txSUCCESS"`, `"txFAILED"`)
    pub result_code: Option<String>,
    /// Extracted Soroban / smart-contract execution result, if applicable
    pub contract_result: Option<ContractExecutionResult>,
    /// `true` when the transaction was both found on-chain and marked successful
    pub execution_confirmed: bool,
    /// Wall-clock time at which this verification was performed
    pub verified_at: Option<SystemTime>,
    /// Human-readable error message when status is `Failed` or `NotFound`
    pub error_message: Option<String>,
}

impl VerificationResponse {
    /// Build a response for a successfully confirmed transaction
    pub fn confirmed(
        hash: impl Into<String>,
        ledger: u32,
        contract_result: Option<ContractExecutionResult>,
    ) -> Self {
        Self {
            transaction_hash: hash.into(),
            status: VerificationStatus::Confirmed,
            successful: Some(true),
            ledger_sequence: Some(ledger),
            ledger_close_time: None,
            fee_charged: None,
            result_code: None,
            contract_result,
            execution_confirmed: true,
            verified_at: Some(SystemTime::now()),
            error_message: None,
        }
    }

    /// Build a response for a transaction that is on-chain but failed
    pub fn tx_failed(hash: impl Into<String>, result_code: impl Into<String>) -> Self {
        let code = result_code.into();
        Self {
            transaction_hash: hash.into(),
            status: VerificationStatus::Failed,
            successful: Some(false),
            ledger_sequence: None,
            ledger_close_time: None,
            fee_charged: None,
            result_code: Some(code.clone()),
            contract_result: None,
            execution_confirmed: false,
            verified_at: Some(SystemTime::now()),
            error_message: Some(format!(
                "Transaction included in ledger but marked as failed (code: {})",
                code
            )),
        }
    }

    /// Build a response for a transaction that could not be found on Horizon
    pub fn not_found(hash: impl Into<String>) -> Self {
        Self {
            transaction_hash: hash.into(),
            status: VerificationStatus::NotFound,
            successful: None,
            ledger_sequence: None,
            ledger_close_time: None,
            fee_charged: None,
            result_code: None,
            contract_result: None,
            execution_confirmed: false,
            verified_at: Some(SystemTime::now()),
            error_message: Some("Transaction not found on-chain".to_string()),
        }
    }

    /// Convenience: is the transaction confirmed?
    pub fn is_confirmed(&self) -> bool {
        self.execution_confirmed
    }

    /// Convenience: did the transaction fail on-chain?
    pub fn is_failed(&self) -> bool {
        self.status == VerificationStatus::Failed
    }

    /// Convenience: was the transaction absent from Horizon?
    pub fn is_not_found(&self) -> bool {
        self.status == VerificationStatus::NotFound
    }
}

/// Extracted result of a Soroban smart-contract execution
///
/// For non-contract (classic) transactions this struct is omitted from the
/// response entirely. When present, `return_value_xdr` holds the raw
/// base64-encoded XDR of the contract's return value, which callers can
/// decode with `stellar-xdr` or the Soroban SDK.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractExecutionResult {
    /// Whether the contract call itself succeeded
    pub success: bool,
    /// Base64-encoded XDR of the Soroban return value (from `result_meta_xdr`)
    pub return_value_xdr: Option<String>,
    /// Human-readable summary of the result
    pub result_description: Option<String>,
    /// Events emitted by the contract during this execution
    pub events: Vec<ContractEvent>,
    /// Per-operation results extracted from the transaction
    pub operation_results: Vec<OperationResult>,
}

impl ContractExecutionResult {
    /// Create a successful result with an optional raw return-value XDR
    pub fn success(return_value_xdr: Option<String>) -> Self {
        Self {
            success: true,
            return_value_xdr,
            result_description: Some("Contract executed successfully".to_string()),
            events: vec![],
            operation_results: vec![],
        }
    }

    /// Create a failed result with a descriptive reason
    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            success: false,
            return_value_xdr: None,
            result_description: Some(reason.into()),
            events: vec![],
            operation_results: vec![],
        }
    }
}

/// A single Soroban contract event emitted during transaction execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractEvent {
    /// Event type as returned by Horizon (`"contract"`, `"system"`, `"diagnostic"`)
    pub event_type: String,
    /// Strkey of the contract that emitted the event (if available)
    pub contract_id: Option<String>,
    /// Event topics — base64-encoded XDR `ScVal` entries
    pub topics: Vec<String>,
    /// Event data — base64-encoded XDR `ScVal` (if present)
    pub data: Option<String>,
}

/// Result summary for a single operation within the transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    /// Zero-based operation index within the transaction
    pub index: u32,
    /// Whether this individual operation succeeded
    pub successful: bool,
    /// Stellar result code string (e.g. `"op_success"`, `"op_underfunded"`)
    pub result_code: String,
    /// Human-readable description of the result
    pub description: String,
}

/// Configuration for the `TransactionVerificationService`
#[derive(Debug, Clone)]
pub struct VerificationConfig {
    /// Base URL for the Horizon REST API
    pub horizon_url: String,
    /// Per-request HTTP timeout
    pub timeout: Duration,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            horizon_url: "https://horizon.stellar.org".to_string(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl VerificationConfig {
    /// Testnet configuration
    pub fn testnet() -> Self {
        Self {
            horizon_url: "https://horizon-testnet.stellar.org".to_string(),
            ..Default::default()
        }
    }

    /// Mainnet configuration (same as `default`)
    pub fn mainnet() -> Self {
        Self::default()
    }

    /// Local standalone network (e.g. `stellar/quickstart`)
    pub fn local() -> Self {
        Self {
            horizon_url: "http://localhost:8000".to_string(),
            timeout: Duration::from_secs(10),
        }
    }

    /// Override the Horizon base URL
    pub fn with_horizon_url(mut self, url: impl Into<String>) -> Self {
        self.horizon_url = url.into();
        self
    }

    /// Override the request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_status_display() {
        assert_eq!(VerificationStatus::Confirmed.to_string(), "confirmed");
        assert_eq!(VerificationStatus::Failed.to_string(), "failed");
        assert_eq!(VerificationStatus::NotFound.to_string(), "not_found");
    }

    #[test]
    fn test_verification_request_defaults() {
        let req = VerificationRequest::new("abc");
        assert_eq!(req.transaction_hash, "abc");
        assert_eq!(req.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_verification_request_custom_timeout() {
        let req = VerificationRequest::new("abc").with_timeout(Duration::from_secs(60));
        assert_eq!(req.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_response_confirmed() {
        let resp = VerificationResponse::confirmed("hash_abc", 100_000, None);
        assert!(resp.is_confirmed());
        assert!(!resp.is_failed());
        assert!(!resp.is_not_found());
        assert_eq!(resp.ledger_sequence, Some(100_000));
        assert_eq!(resp.successful, Some(true));
    }

    #[test]
    fn test_response_tx_failed() {
        let resp = VerificationResponse::tx_failed("hash_abc", "txFAILED");
        assert!(!resp.is_confirmed());
        assert!(resp.is_failed());
        assert_eq!(resp.result_code.as_deref(), Some("txFAILED"));
        assert!(resp.error_message.is_some());
    }

    #[test]
    fn test_response_not_found() {
        let resp = VerificationResponse::not_found("hash_abc");
        assert!(!resp.is_confirmed());
        assert!(resp.is_not_found());
        assert!(resp.successful.is_none());
    }

    #[test]
    fn test_contract_result_success() {
        let r = ContractExecutionResult::success(Some("AAAA".into()));
        assert!(r.success);
        assert_eq!(r.return_value_xdr.as_deref(), Some("AAAA"));
    }

    #[test]
    fn test_contract_result_failed() {
        let r = ContractExecutionResult::failed("invoke_error");
        assert!(!r.success);
        assert!(r.return_value_xdr.is_none());
    }

    #[test]
    fn test_verification_config_testnet() {
        let cfg = VerificationConfig::testnet();
        assert!(cfg.horizon_url.contains("testnet"));
    }

    #[test]
    fn test_verification_config_mainnet() {
        let cfg = VerificationConfig::mainnet();
        assert!(!cfg.horizon_url.contains("testnet"));
    }
}
