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

/// Issue #136 — end-to-end conversion of a realistic donation payload through
/// `method_args_from_json` + `build_stellar_args` (pure functions only; no
/// `stellar` binary or network access, so `make test` stays hermetic).
#[test]
fn test_invoke_args_json_to_stellar_argv_integration() {
    use orbitchain_tools::invoke::{
        build_stellar_args, method_args_from_json, parse_invoke_args, validate_contract_id,
    };

    let contract_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    let donor = "GDVEU3DD4KOFECV66VIHWEZOYX4ZKR3WV27L464SIIPOU2IUI3JCZA57";
    // Realistic donation payload: Address, u64 id, i128-as-string amount,
    // Vec of milestone targets.
    let args_json = format!(
        r#"{{"donor":"{}","campaign_id":7,"amount":"170141183460469231731687303715884105727","targets":[100,200]}}"#,
        donor
    );

    let cli_args: Vec<String> = [
        "--id",
        contract_id,
        "--method",
        "donate",
        "--args-json",
        &args_json,
        "--network",
        "testnet",
        "--send",
        "no",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let opts = parse_invoke_args(&cli_args).expect("flags should parse");
    validate_contract_id(&opts.contract_id).expect("contract id should validate");

    let method_args =
        method_args_from_json(opts.args_json.as_deref().unwrap()).expect("json should convert");

    let network =
        orbitchain_tools::environment_config::EnvironmentConfig::default_for_testnet().testnet;
    let argv = build_stellar_args(&opts, &network, &method_args);

    // Fixed preamble ordering.
    assert_eq!(
        &argv[..8],
        &[
            "contract".to_string(),
            "invoke".to_string(),
            "--id".to_string(),
            contract_id.to_string(),
            "--rpc-url".to_string(),
            network.rpc_url.clone(),
            "--network-passphrase".to_string(),
            network.network_passphrase.clone(),
        ]
    );

    // --send passthrough, then `--` separator, then method and its args.
    let sep = argv.iter().position(|a| a == "--").expect("`--` separator");
    assert!(argv[..sep]
        .windows(2)
        .any(|w| w[0] == "--send" && w[1] == "no"));
    assert_eq!(argv[sep + 1], "donate");

    let method_argv = &argv[sep + 2..];
    let has_pair = |flag: &str, value: &str| {
        method_argv
            .windows(2)
            .any(|w| w[0] == flag && w[1] == value)
    };
    assert!(has_pair("--donor", donor));
    assert!(has_pair("--campaign_id", "7"));
    assert!(has_pair(
        "--amount",
        "170141183460469231731687303715884105727"
    ));
    assert!(has_pair("--targets", "[100,200]"));
    assert_eq!(method_argv.len(), 8);
}

// Module references for the test
mod signing_request {
    pub use orbitchain_tools::signing_request::*;
}

mod response_handler {
    pub use orbitchain_tools::response_handler::*;
}
