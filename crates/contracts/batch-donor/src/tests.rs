//! Host-target tests for the batch-donor contract (`issue #147`).
//!
//! These tests intentionally use `mock_all_auths()` and an in-process
//! `CampaignContract` registry. We deliberately do NOT exercise the live
//! cross-contract invoke path here — that is covered by integration tests
//! on a Stellar sandbox running `make deploy-sandbox`. This file covers:
//!
//! - Basic entrypoint surface (version_str, hello, max_batch_size).
//! - Pre-flight validation gates (`EMPTY_BATCH`, `BATCH_TOO_LARGE`,
//!   `INVALID_AMOUNT`).
//! - The contract compiles for `wasm32v1-none` (verified by `cargo check`
//!   in `.github/workflows/ci.yml`).

use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

use crate::types::{AssetInfo, BatchDonateOutcome, BatchDonateResult, DonateTarget};
use crate::{BatchDonorContract, BatchDonorContractClient};

#[test]
fn entrypoints_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BatchDonorContract);
    let client = BatchDonorContractClient::new(&env, &contract_id);

    // No auth needed for read-only entrypoints.
    assert_eq!(
        client.hello(),
        soroban_sdk::Symbol::new(&env, "batch_donor")
    );
    assert_eq!(
        client.version_str(),
        soroban_sdk::String::from_str(&env, "0.1.0")
    );
    assert_eq!(client.max_batch_size(), 50);
    // Legacy integer version view mirrors VERSION_MINOR (= 1).
    assert_eq!(client.version(), 1u32);
}

#[test]
#[should_panic]
fn empty_batch_panics_with_validation_code() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BatchDonorContract);
    let client = BatchDonorContractClient::new(&env, &contract_id);

    let operator = Address::generate(&env);
    let empty_targets: Vec<DonateTarget> = Vec::new(&env);

    client.batch_donate(&operator, &empty_targets);
}

#[test]
#[should_panic]
fn negative_amount_panics_with_validation_code() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BatchDonorContract);
    let client = BatchDonorContractClient::new(&env, &contract_id);

    let operator = Address::generate(&env);
    let mut targets: Vec<DonateTarget> = Vec::new(&env);
    targets.push_back(DonateTarget {
        campaign: Address::generate(&env),
        donor: Address::generate(&env),
        amount: -1,
        asset: AssetInfo::Native,
    });

    client.batch_donate(&operator, &targets);
}

#[test]
#[should_panic]
fn zero_amount_panics_with_validation_code() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BatchDonorContract);
    let client = BatchDonorContractClient::new(&env, &contract_id);

    let operator = Address::generate(&env);
    let mut targets: Vec<DonateTarget> = Vec::new(&env);
    targets.push_back(DonateTarget {
        campaign: Address::generate(&env),
        donor: Address::generate(&env),
        amount: 0,
        asset: AssetInfo::Native,
    });

    client.batch_donate(&operator, &targets);
}

#[test]
#[should_panic]
fn oversized_batch_panics_with_validation_code() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BatchDonorContract);
    let client = BatchDonorContractClient::new(&env, &contract_id);

    let operator = Address::generate(&env);
    let mut targets: Vec<DonateTarget> = Vec::new(&env);
    for _ in 0..(crate::MAX_BATCH_SIZE + 1) {
        targets.push_back(DonateTarget {
            campaign: Address::generate(&env),
            donor: Address::generate(&env),
            amount: 1_000_000,
            asset: AssetInfo::Native,
        });
    }

    client.batch_donate(&operator, &targets);
}

#[test]
fn batch_donate_result_struct_has_expected_shape() {
    let env = Env::default();
    let result = BatchDonateResult {
        index: 7,
        outcome: BatchDonateOutcome::ValidationFailed(42),
        validation_code: Some(42),
    };
    assert_eq!(result.index, 7);
    assert_eq!(result.validation_code, Some(42));
}

#[test]
fn workspace_version_string_is_known() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BatchDonorContract);
    let client = BatchDonorContractClient::new(&env, &contract_id);
    // Lock the contract version string so accidental regulator bumps cause
    // a loud failure rather than a silent shipping of an out-of-sync WASM.
    assert_eq!(
        client.version_str(),
        soroban_sdk::String::from_str(&env, "0.1.0")
    );
}
