//! Signed transaction response handler.
//!
//! Parses wallet signing responses, validates signed transactions,
//! and persists/loads them from JSON files for later submission.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;

/// Represents a signed transaction response from a wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub request_id: String,
    pub transaction_xdr: String,
    pub signed_at: u64,
    pub signer: String,
    pub status: TransactionStatus,
}

/// Status of a signed transaction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    #[serde(rename = "signed")]
    Signed,
    #[serde(rename = "submitted")]
    Submitted,
    #[serde(rename = "confirmed")]
    Confirmed,
    #[serde(rename = "failed")]
    Failed,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Signed => write!(f, "Signed"),
            TransactionStatus::Submitted => write!(f, "Submitted"),
            TransactionStatus::Confirmed => write!(f, "Confirmed"),
            TransactionStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// Handler for processing and validating signed transactions.
/// All methods are static — no instantiation required.
pub struct ResponseHandler;

impl ResponseHandler {
    /// Parse signed transaction from JSON response.
    pub fn parse_response(response_json: &str) -> Result<SignedTransaction> {
        let parsed: serde_json::Value =
            serde_json::from_str(response_json).context("Failed to parse response JSON")?;

        Ok(SignedTransaction {
            request_id: parsed["requestId"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing requestId"))?
                .to_string(),
            transaction_xdr: parsed["xdr"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing transaction XDR"))?
                .to_string(),
            signed_at: parsed["signedAt"]
                .as_u64()
                .unwrap_or_else(|| chrono::Local::now().timestamp() as u64),
            signer: parsed["signer"].as_str().unwrap_or("unknown").to_string(),
            status: TransactionStatus::Signed,
        })
    }

    /// Validate a signed transaction.
    pub fn validate(tx: &SignedTransaction) -> Result<()> {
        if tx.request_id.is_empty() {
            return Err(anyhow!("Request ID cannot be empty"));
        }

        if tx.transaction_xdr.is_empty() {
            return Err(anyhow!("Transaction XDR cannot be empty"));
        }

        if tx.signer.is_empty() {
            return Err(anyhow!("Signer address cannot be empty"));
        }

        Ok(())
    }

    /// Save signed transaction to file.
    pub fn save_to_file(tx: &SignedTransaction, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(tx).context("Failed to serialize transaction")?;

        fs::write(path, json).context(format!("Failed to write transaction to {}", path))?;

        Ok(())
    }

    /// Load signed transaction from file.
    pub fn load_from_file(path: &str) -> Result<SignedTransaction> {
        let content = fs::read_to_string(path)
            .context(format!("Failed to read transaction from {}", path))?;

        serde_json::from_str(&content).context("Failed to deserialize transaction")
    }

    /// Process wallet response and return signed transaction
    pub fn process_response(response: &str) -> Result<ProcessedResponse> {
        let signed_tx = Self::parse_response(response)?;
        Self::validate(&signed_tx)?;

        Ok(ProcessedResponse {
            signed_transaction: signed_tx,
            validation_errors: vec![],
            warnings: vec![],
        })
    }
}

/// Result of processing a wallet response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedResponse {
    pub signed_transaction: SignedTransaction,
    pub validation_errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ProcessedResponse {
    /// Check if response is valid for submission
    pub fn is_valid(&self) -> bool {
        self.validation_errors.is_empty()
    }

    /// Display response details
    pub fn display(&self) {
        println!("✅ Transaction Signed Successfully");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Request ID:    {}", self.signed_transaction.request_id);
        println!("Signer:        {}", self.signed_transaction.signer);
        println!("Status:        {}", self.signed_transaction.status);
        println!("Signed At:     {}", self.signed_transaction.signed_at);
        println!();

        if !self.warnings.is_empty() {
            println!("⚠️  Warnings:");
            for warning in &self.warnings {
                println!("   - {}", warning);
            }
            println!();
        }

        if !self.validation_errors.is_empty() {
            println!("❌ Validation Errors:");
            for error in &self.validation_errors {
                println!("   - {}", error);
            }
        } else {
            println!("Ready for submission");
        }
    }

    /// Export as JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize response")
    }
}

/// Transaction submission result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionResult {
    pub request_id: String,
    pub transaction_hash: String,
    pub status: TransactionStatus,
    pub submitted_at: u64,
    pub ledger: Option<u32>,
    pub result_code: String,
}

impl SubmissionResult {
    /// Display submission result
    pub fn display(&self) {
        println!("📤 Submission Result");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Request ID:     {}", self.request_id);
        println!("Tx Hash:        {}", self.transaction_hash);
        println!("Status:         {}", self.status);
        println!("Submitted At:   {}", self.submitted_at);

        if let Some(ledger) = self.ledger {
            println!("Ledger:         {}", ledger);
        }

        println!("Result Code:    {}", self.result_code);

        if self.status == TransactionStatus::Confirmed {
            println!("✅ Transaction confirmed!");
        } else if self.status == TransactionStatus::Failed {
            println!("❌ Transaction failed");
        }
    }
}

/// Helper for building responses (useful for testing)
pub struct ResponseBuilder;

impl ResponseBuilder {
    /// Create a test response JSON
    pub fn build_response(request_id: String, xdr: String, signer: String) -> String {
        json!({
            "requestId": request_id,
            "xdr": xdr,
            "signer": signer,
            "signedAt": chrono::Local::now().timestamp(),
        })
        .to_string()
    }

    /// Create a test signed transaction
    pub fn build_signed_transaction(
        request_id: String,
        xdr: String,
        signer: String,
    ) -> SignedTransaction {
        SignedTransaction {
            request_id,
            transaction_xdr: xdr,
            signed_at: chrono::Local::now().timestamp() as u64,
            signer,
            status: TransactionStatus::Signed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_response() {
        let response = json!({
            "requestId": "req_123",
            "xdr": "AAAAAA==test",
            "signer": "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W",
            "signedAt": 1234567890
        })
        .to_string();

        let result = ResponseHandler::parse_response(&response);
        assert!(result.is_ok());

        let tx = result.unwrap();
        assert_eq!(tx.request_id, "req_123");
        assert_eq!(tx.transaction_xdr, "AAAAAA==test");
    }

    #[test]
    fn test_signed_transaction_validation() {
        let tx = SignedTransaction {
            request_id: "req_123".to_string(),
            transaction_xdr: "AAAAAA==".to_string(),
            signed_at: 0,
            signer: "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W".to_string(),
            status: TransactionStatus::Signed,
        };

        assert!(ResponseHandler::validate(&tx).is_ok());
    }

    #[test]
    fn test_processed_response_validity() {
        let response = ProcessedResponse {
            signed_transaction: SignedTransaction {
                request_id: "req_123".to_string(),
                transaction_xdr: "AAAAAA==".to_string(),
                signed_at: 0,
                signer: "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W".to_string(),
                status: TransactionStatus::Signed,
            },
            validation_errors: vec![],
            warnings: vec![],
        };

        assert!(response.is_valid());
    }

    #[test]
    fn test_response_builder() {
        let response = ResponseBuilder::build_response(
            "req_123".to_string(),
            "AAAAAA==test".to_string(),
            "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W".to_string(),
        );

        assert!(response.contains("req_123"));
        assert!(response.contains("AAAAAA==test"));
    }
}
