//! Transaction Verification Service
//!
//! Verifies on-chain transaction execution by:
//! 1. Fetching the transaction from Horizon by hash
//! 2. Validating its `successful` status field
//! 3. Extracting the Soroban / contract execution result (return value XDR + events)
//! 4. Returning a typed `VerificationResponse` that confirms or denies on-chain execution

use super::error::{VerificationError, VerificationResult};
use super::types::{
    ContractEvent, ContractExecutionResult, OperationResult, VerificationConfig,
    VerificationRequest, VerificationResponse,
};
use log::{debug, info, warn};
use reqwest::{Client, StatusCode};
use std::time::Duration;

/// Service for verifying Stellar transactions on-chain via Horizon
pub struct TransactionVerificationService {
    config: VerificationConfig,
    http_client: Client,
}

impl TransactionVerificationService {
    /// Create a new service with mainnet defaults
    pub fn new() -> VerificationResult<Self> {
        Self::with_config(VerificationConfig::default())
    }

    /// Create a service with a custom configuration
    pub fn with_config(config: VerificationConfig) -> VerificationResult<Self> {
        let http_client = Client::builder()
            .timeout(config.timeout)
            .user_agent("stellaraid-verifier/1.0")
            .build()
            .map_err(|e| VerificationError::InternalError {
                message: format!("Failed to build HTTP client: {}", e),
            })?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Create a service pre-configured for Stellar Testnet
    pub fn testnet() -> VerificationResult<Self> {
        Self::with_config(VerificationConfig::testnet())
    }

    /// Create a service pre-configured for Stellar Mainnet
    pub fn mainnet() -> VerificationResult<Self> {
        Self::with_config(VerificationConfig::mainnet())
    }

    /// Inspect the current configuration
    pub fn config(&self) -> &VerificationConfig {
        &self.config
    }

    // -------------------------------------------------------------------------
    // Public API
    // -------------------------------------------------------------------------

    /// Verify a transaction on-chain.
    ///
    /// Workflow:
    /// 1. **Fetch** – `GET /transactions/{hash}` on Horizon.
    /// 2. **Validate** – inspect the `successful` boolean in the response.
    /// 3. **Extract** – parse `result_meta_xdr` and any Soroban-specific fields
    ///    into a [`ContractExecutionResult`].
    /// 4. **Confirm** – return a [`VerificationResponse`] with
    ///    `execution_confirmed = true` only when the transaction is both
    ///    present on-chain and marked successful.
    pub async fn verify(
        &self,
        request: VerificationRequest,
    ) -> VerificationResult<VerificationResponse> {
        let hash = &request.transaction_hash;

        // Step 0: Validate hash format before making a network call
        if !is_valid_tx_hash(hash) {
            return Err(VerificationError::InvalidHash {
                hash: hash.clone(),
            });
        }

        info!("Verifying transaction on-chain: {}", hash);

        // Step 1: Fetch the transaction by hash
        let tx_json = self.fetch_transaction(hash).await?;

        // Step 2: Validate success status
        let successful = tx_json
            .get("successful")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let ledger = tx_json
            .get("ledger")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        let ledger_close_time = tx_json
            .get("created_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let fee_charged = tx_json
            .get("fee_charged")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let result_code = tx_json
            .get("result_code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if !successful {
            let code = result_code.clone().unwrap_or_else(|| "tx_failed".to_string());
            warn!("Transaction {} on-chain but failed (code: {})", hash, code);

            let mut resp = VerificationResponse::tx_failed(hash.clone(), code);
            resp.ledger_sequence = ledger;
            resp.ledger_close_time = ledger_close_time;
            resp.fee_charged = fee_charged;
            return Ok(resp);
        }

        // Step 3: Extract contract / Soroban result
        let contract_result = self.extract_contract_result(&tx_json);

        // Step 4: Confirm execution and build the response
        let ledger_seq = ledger.unwrap_or(0);
        info!(
            "Transaction {} confirmed in ledger {}",
            hash, ledger_seq
        );

        let mut resp = VerificationResponse::confirmed(hash.clone(), ledger_seq, contract_result);
        resp.ledger_close_time = ledger_close_time;
        resp.fee_charged = fee_charged;
        resp.result_code = result_code;

        Ok(resp)
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// `GET /transactions/{hash}` — returns the raw Horizon JSON on success.
    async fn fetch_transaction(&self, hash: &str) -> VerificationResult<serde_json::Value> {
        let url = format!("{}/transactions/{}", self.config.horizon_url, hash);
        debug!("Fetching transaction from Horizon: {}", url);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    VerificationError::Timeout {
                        duration: self.config.timeout,
                    }
                } else if e.is_connect() {
                    VerificationError::NetworkError {
                        message: format!("Connection failed: {}", e),
                    }
                } else {
                    VerificationError::NetworkError {
                        message: e.to_string(),
                    }
                }
            })?;

        let status = response.status();

        match status {
            StatusCode::OK => {
                let json = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| VerificationError::InvalidResponse {
                        message: format!("Failed to parse Horizon response: {}", e),
                    })?;
                Ok(json)
            }

            StatusCode::NOT_FOUND => Err(VerificationError::NotFound {
                hash: hash.to_string(),
            }),

            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(Duration::from_secs(60));
                Err(VerificationError::RateLimited { retry_after })
            }

            s if s.is_server_error() => {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown server error".to_string());
                Err(VerificationError::ServerError {
                    status: s.as_u16(),
                    message: body,
                })
            }

            s => {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unexpected response".to_string());
                Err(VerificationError::ServerError {
                    status: s.as_u16(),
                    message: body,
                })
            }
        }
    }

    /// Parse the Horizon transaction JSON and build a [`ContractExecutionResult`].
    ///
    /// Soroban contract invocations embed their execution data in:
    /// - `result_xdr`      – the `TransactionResult` XDR (contains the op result)
    /// - `result_meta_xdr` – the `TransactionMeta` XDR which, for Soroban v3+,
    ///                       contains `SorobanTransactionMeta` with the return
    ///                       value and emitted events.
    ///
    /// We capture both raw fields so callers can decode them with `stellar-xdr`.
    /// When Horizon also exposes a parsed `soroban_meta` object we extract the
    /// events directly.
    ///
    /// Returns `None` for classic (non-contract) transactions that carry no XDR.
    fn extract_contract_result(
        &self,
        tx_json: &serde_json::Value,
    ) -> Option<ContractExecutionResult> {
        let result_meta_xdr = tx_json
            .get("result_meta_xdr")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Use result_xdr as a secondary signal that there is something to decode
        let has_result_xdr = tx_json
            .get("result_xdr")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        // If neither XDR field is present this is a no-op / classic tx
        if result_meta_xdr.is_none() && !has_result_xdr {
            return None;
        }

        // Build per-operation results (synthetic — full detail requires a
        // separate `/transactions/{hash}/operations` call)
        let operation_results = self.extract_operation_results(tx_json);

        // Prefer the decoded `soroban_meta` when Horizon provides it; otherwise
        // hand the raw XDR back to the caller for off-line decoding.
        let events = self.extract_soroban_events(tx_json).unwrap_or_default();

        let mut result = ContractExecutionResult::success(result_meta_xdr);
        result.operation_results = operation_results;
        result.events = events;

        Some(result)
    }

    /// Build synthetic [`OperationResult`] entries from the transaction envelope's
    /// `operation_count`.  Full per-operation codes require a separate Horizon
    /// call to `/transactions/{hash}/operations`.
    fn extract_operation_results(&self, tx_json: &serde_json::Value) -> Vec<OperationResult> {
        let op_count = tx_json
            .get("operation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        (0..op_count)
            .map(|i| OperationResult {
                index: i,
                successful: true,
                result_code: "op_success".to_string(),
                description: "Operation executed successfully".to_string(),
            })
            .collect()
    }

    /// Extract parsed Soroban events from the optional `soroban_meta` object
    /// that newer Horizon versions include alongside `result_meta_xdr`.
    fn extract_soroban_events(&self, tx_json: &serde_json::Value) -> Option<Vec<ContractEvent>> {
        let events_json = tx_json
            .get("soroban_meta")
            .and_then(|m| m.get("events"))
            .and_then(|e| e.as_array())?;

        let events = events_json
            .iter()
            .filter_map(|event| {
                let event_type = event
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("contract")
                    .to_string();

                let contract_id = event
                    .get("contract_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let topics = event
                    .get("topic")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let data = event
                    .get("value")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                Some(ContractEvent {
                    event_type,
                    contract_id,
                    topics,
                    data,
                })
            })
            .collect();

        Some(events)
    }
}

impl Default for TransactionVerificationService {
    fn default() -> Self {
        Self::new().expect("Failed to create TransactionVerificationService with default config")
    }
}

// ---------------------------------------------------------------------------
// Helper utilities
// ---------------------------------------------------------------------------

/// A valid Stellar transaction hash is exactly 64 lowercase hex characters.
pub fn is_valid_tx_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_verification::types::VerificationRequest;

    #[test]
    fn test_service_new() {
        assert!(TransactionVerificationService::new().is_ok());
    }

    #[test]
    fn test_service_testnet() {
        let svc = TransactionVerificationService::testnet().unwrap();
        assert!(svc.config().horizon_url.contains("testnet"));
    }

    #[test]
    fn test_service_mainnet() {
        let svc = TransactionVerificationService::mainnet().unwrap();
        assert!(svc.config().horizon_url.contains("stellar.org"));
        assert!(!svc.config().horizon_url.contains("testnet"));
    }

    #[test]
    fn test_service_custom_config() {
        let config = VerificationConfig::default()
            .with_horizon_url("https://custom.horizon.example.com")
            .with_timeout(Duration::from_secs(45));

        let svc = TransactionVerificationService::with_config(config).unwrap();
        assert_eq!(
            svc.config().horizon_url,
            "https://custom.horizon.example.com"
        );
        assert_eq!(svc.config().timeout, Duration::from_secs(45));
    }

    // --- Hash validation ---

    #[test]
    fn test_valid_tx_hash() {
        let hash = "a3d7e8f9b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8";
        assert!(is_valid_tx_hash(hash));
    }

    #[test]
    fn test_invalid_hash_too_short() {
        assert!(!is_valid_tx_hash("abc123def"));
    }

    #[test]
    fn test_invalid_hash_too_long() {
        let too_long = "a".repeat(65);
        assert!(!is_valid_tx_hash(&too_long));
    }

    #[test]
    fn test_invalid_hash_non_hex_chars() {
        // 'z' is not a valid hex character
        let bad = "z3d7e8f9b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8";
        assert!(!is_valid_tx_hash(bad));
    }

    #[test]
    fn test_invalid_hash_empty() {
        assert!(!is_valid_tx_hash(""));
    }

    // --- verify() input validation (no network) ---

    #[tokio::test]
    async fn test_verify_rejects_invalid_hash() {
        let svc = TransactionVerificationService::new().unwrap();
        let req = VerificationRequest::new("not-a-valid-hash");
        let result = svc.verify(req).await;
        assert!(matches!(result, Err(VerificationError::InvalidHash { .. })));
    }

    // --- extract_contract_result ---

    #[test]
    fn test_extract_contract_result_none_when_no_xdr() {
        let svc = TransactionVerificationService::new().unwrap();
        let json = serde_json::json!({
            "hash": "abc",
            "successful": true,
            "ledger": 123
        });
        assert!(svc.extract_contract_result(&json).is_none());
    }

    #[test]
    fn test_extract_contract_result_with_meta_xdr() {
        let svc = TransactionVerificationService::new().unwrap();
        let json = serde_json::json!({
            "hash": "abc",
            "successful": true,
            "result_meta_xdr": "AAAAAAAAAAA=",
            "operation_count": 1
        });
        let result = svc.extract_contract_result(&json);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.success);
        assert_eq!(r.return_value_xdr.as_deref(), Some("AAAAAAAAAAA="));
        assert_eq!(r.operation_results.len(), 1);
        assert_eq!(r.operation_results[0].index, 0);
        assert_eq!(r.operation_results[0].result_code, "op_success");
    }

    #[test]
    fn test_extract_contract_result_with_result_xdr_only() {
        let svc = TransactionVerificationService::new().unwrap();
        let json = serde_json::json!({
            "hash": "abc",
            "successful": true,
            "result_xdr": "AAAAAA==",
            "operation_count": 2
        });
        let result = svc.extract_contract_result(&json);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.operation_results.len(), 2);
    }

    #[test]
    fn test_extract_soroban_events_present() {
        let svc = TransactionVerificationService::new().unwrap();
        let json = serde_json::json!({
            "soroban_meta": {
                "events": [
                    {
                        "type": "contract",
                        "contract_id": "CABC123",
                        "topic": ["AAAA", "BBBB"],
                        "value": "CCCC"
                    }
                ]
            }
        });
        let events = svc.extract_soroban_events(&json);
        assert!(events.is_some());
        let events = events.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "contract");
        assert_eq!(events[0].contract_id.as_deref(), Some("CABC123"));
        assert_eq!(events[0].topics, vec!["AAAA", "BBBB"]);
        assert_eq!(events[0].data.as_deref(), Some("CCCC"));
    }

    #[test]
    fn test_extract_soroban_events_absent() {
        let svc = TransactionVerificationService::new().unwrap();
        let json = serde_json::json!({ "hash": "abc" });
        assert!(svc.extract_soroban_events(&json).is_none());
    }

    #[test]
    fn test_extract_operation_results_zero() {
        let svc = TransactionVerificationService::new().unwrap();
        let json = serde_json::json!({ "operation_count": 0 });
        assert!(svc.extract_operation_results(&json).is_empty());
    }

    #[test]
    fn test_extract_operation_results_multiple() {
        let svc = TransactionVerificationService::new().unwrap();
        let json = serde_json::json!({ "operation_count": 3 });
        let ops = svc.extract_operation_results(&json);
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].index, 0);
        assert_eq!(ops[2].index, 2);
        assert!(ops.iter().all(|o| o.successful));
    }
}
