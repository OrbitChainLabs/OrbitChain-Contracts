//! Integration test covering the signing-request → response-handler round-trip.
//!
//! Simulates building a signing request, serializing to JSON, simulating a wallet
//! signing response, processing and validating the result, and persisting/loading.

use std::fs;

#[test]
fn test_signing_and_response_integration() {
    // Simulate the complete flow of building a signing request and handling the response

    // Step 1: Build a signing request
    let request_xdr = "AAAAAgAAAADDRVZm3Wgf40kMCwbWI6txY5T7PX0J8p5hJF3J+VBDAAAAAAAAA".to_string();
    let request =
        signing_request::SigningRequestBuilder::new(request_xdr, Some("testnet".to_string()))
            .expect("Failed to create builder")
            .with_description("Test donation to campaign #1".to_string())
            .build()
            .expect("Failed to build request");

    // Verify request structure
    assert!(!request.id.is_empty());
    assert_eq!(request.network, "testnet");
    assert_eq!(request.description, "Test donation to campaign #1");

    // Step 2: Serialize request to JSON for wallet
    let request_json = request.to_json().expect("Failed to serialize");
    assert!(request_json.contains("testnet"));

    // Step 3: Simulate wallet signing response
    let response_json = format!(
        r#"{{
        "requestId": "{}",
        "xdr": "AAAAAgAAAADDRVZm3Wgf40kMCwbWI6txY5T7PX0J8p5hJF3J+VBDAAAAAAAAA==",
        "signer": "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W",
        "signedAt": 1234567890
    }}"#,
        request.id
    );

    // Step 4: Process the response
    let processed = response_handler::ResponseHandler::process_response(&response_json)
        .expect("Failed to process response");

    assert!(processed.is_valid());
    assert_eq!(processed.signed_transaction.request_id, request.id);
    assert_eq!(
        processed.signed_transaction.signer,
        "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W"
    );

    // Step 5: Save signed transaction for later submission
    let temp_file = "/tmp/test_signed_tx.json";
    response_handler::ResponseHandler::save_to_file(&processed.signed_transaction, temp_file)
        .expect("Failed to save transaction");

    // Step 6: Load and verify saved transaction
    let loaded_tx = response_handler::ResponseHandler::load_from_file(temp_file)
        .expect("Failed to load transaction");

    assert_eq!(loaded_tx.request_id, request.id);
    assert_eq!(loaded_tx.signer, processed.signed_transaction.signer);

    // Cleanup
    let _ = fs::remove_file(temp_file);
}

// Module references for the test
mod signing_request {
    pub use orbitchain_tools::signing_request::*;
}

mod response_handler {
    pub use orbitchain_tools::response_handler::*;
}
