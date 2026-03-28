//! Transaction Verification Service
//!
//! Provides on-chain verification of Stellar transactions via the Horizon REST API.
//!
//! # Workflow
//!
//! ```text
//! VerificationRequest (tx hash)
//!        │
//!        ▼
//! TransactionVerificationService::verify()
//!        │
//!        ├─ 1. Fetch transaction by hash   GET /transactions/{hash}
//!        ├─ 2. Validate success status     tx_json["successful"] == true
//!        ├─ 3. Extract contract result     result_meta_xdr + soroban_meta events
//!        └─ 4. Confirm execution           VerificationResponse { execution_confirmed }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use crate::transaction_verification::{
//!     TransactionVerificationService, VerificationRequest,
//! };
//!
//! #[tokio::main]
//! async fn main() {
//!     let service = TransactionVerificationService::testnet().unwrap();
//!     let request = VerificationRequest::new(
//!         "a3d7e8f9b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8",
//!     );
//!     match service.verify(request).await {
//!         Ok(resp) if resp.is_confirmed() => println!("Transaction confirmed!"),
//!         Ok(resp)                        => println!("Status: {}", resp.status),
//!         Err(e)                          => eprintln!("Error: {}", e),
//!     }
//! }
//! ```

pub mod error;
pub mod service;
pub mod types;

pub use error::{VerificationError, VerificationResult};
pub use service::{is_valid_tx_hash, TransactionVerificationService};
pub use types::{
    ContractEvent, ContractExecutionResult, OperationResult, VerificationConfig,
    VerificationRequest, VerificationResponse, VerificationStatus,
};
