//! Tests for the `release_milestone` function.
//!
//! **Strategy:** Because `release_milestone` does cross-contract token transfers,
//! it cannot be tested via `CampaignContract::` direct calls inside `as_contract()`
//! (auth frames break), nor via `CampaignContractClient` outside `as_contract()`
//! (mock token storage isn't visible to the Client's invocation context in SDK 26.x).
//!
//! Solution:
//! - **Business-logic tests** call `crate::release_milestone::release_milestone()`
//!   directly inside `with_contract()`. Auth is already handled upstream by the
//!   `#[contractimpl]` wrapper in `lib.rs`, so the module function is auth-free.
//! - Tests that exercise token transfers call `mint_tokens_to_contract()` (which
//!   needs `mock_all_auths()`) BEFORE `with_contract()`.
//! - **Auth-rejection test** uses the `CampaignContractClient` without
//!   `mock_all_auths()`, and sets up campaign storage without token minting
//!   (auth fails before reaching token ops).
//! - **Integration tests** (in `integration_tests.rs`) cover the full lifecycle
//!   with Client-based auth.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, Env, Vec, String, BytesN};
use soroban_sdk::token::StellarAssetClient;

use crate::types::{CampaignStatus, StellarAsset, MilestoneData, MilestoneStatus, CampaignData};
use crate::storage::{get_milestone, set_campaign, set_milestone};
use crate::CampaignContractClient;
use super::with_contract;

/// Base ledger timestamp (1 year in seconds).
const BASE: u64 = 86400 * 365;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Creates a campaign in Active state with the given parameters.
/// Registers a mock token contract (for cross-contract calls).
/// Does NOT mint tokens — call `mint_tokens_to_contract()` separately if
/// `release_milestone` will actually execute token transfers.
fn create_test_campaign(env: &Env, creator: &Address, milestone_count: u32) {
    // Pass an explicit admin address so the mock SAC stores admin storage properly
    let token_admin = Address::generate(env);
    let token_issuer = env.register_stellar_asset_contract(token_admin.clone());

    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(token_issuer),
    });

    let campaign = CampaignData {
        creator: creator.clone(),
        goal_amount: 3000,
        raised_amount: 3000,
        end_time: env.ledger().timestamp() + 86_400,
        status: CampaignStatus::Active,
        accepted_assets: assets,
        milestone_count,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    };
    set_campaign(env, &campaign);
}

/// Initializes the mock SAC and mints tokens to the current contract address
/// so that `balance()` and `transfer()` calls inside `release_milestone` don't
/// panic with `Storage(MissingValue)` in the mock token.
///
/// The mock SAC registered by `register_stellar_asset_contract(admin)` stores
/// the admin, making `StellarAssetClient::mint()` work with `mock_all_auths()`.
///
/// Requires `env.mock_all_auths()` to be called before `with_contract()`.
fn mint_tokens_to_contract(env: &Env) {
    let campaign = crate::storage::get_campaign(env).unwrap();
    if let Some(asset) = campaign.accepted_assets.first() {
        if let Some(issuer) = &asset.issuer {
            let token_admin = StellarAssetClient::new(env, issuer);
            token_admin.mint(&env.current_contract_address(), &10_000_000i128);
        }
    }
}

/// Creates a milestone with the given index, target, and status.
fn create_test_milestone(
    env: &Env,
    index: u32,
    target_amount: i128,
    status: MilestoneStatus,
) {
    let milestone = MilestoneData {
        index,
        target_amount,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[0u8; 32]),
        status,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    };
    set_milestone(env, index, &milestone);
}

/// Creates a simple campaign with one unlocked milestone AND mints tokens.
/// For tests that actually execute token transfers.
fn setup_single_milestone_campaign_with_funding(env: &Env) {
    let creator = Address::generate(env);
    create_test_campaign(env, &creator, 1);
    mint_tokens_to_contract(env);
    create_test_milestone(env, 0, 3000, MilestoneStatus::Unlocked);
}

/// Creates a campaign accepting `asset_count` distinct assets, mints
/// `funding_per_asset` tokens of each into the contract, and returns the
/// list of token issuer addresses in the same order as `accepted_assets`.
fn create_multi_asset_campaign_with_funding(
    env: &Env,
    creator: &Address,
    milestone_count: u32,
    asset_count: u32,
    funding_per_asset: i128,
) -> Vec<Address> {
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    let mut issuers: Vec<Address> = Vec::new(env);

    for i in 0..asset_count {
        let token_admin = Address::generate(env);
        let token_issuer = env.register_stellar_asset_contract(token_admin.clone());
        let code = match i {
            0 => "XLM",
            1 => "USDC",
            _ => "EURC",
        };
        assets.push_back(StellarAsset {
            asset_code: String::from_str(env, code),
            issuer: Some(token_issuer.clone()),
        });
        issuers.push_back(token_issuer.clone());

        let token_admin_client = StellarAssetClient::new(env, &token_issuer);
        token_admin_client.mint(&env.current_contract_address(), &funding_per_asset);
    }

    let campaign = CampaignData {
        creator: creator.clone(),
        goal_amount: 3000,
        raised_amount: 3000,
        end_time: env.ledger().timestamp() + 86_400,
        status: CampaignStatus::Active,
        accepted_assets: assets,
        milestone_count,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    };
    set_campaign(env, &campaign);

    issuers
}

/// Creates a simple campaign with one unlocked milestone WITHOUT minting tokens.
/// For tests that panic before reaching token transfers.
fn setup_single_milestone_campaign_no_funding(env: &Env) {
    let creator = Address::generate(env);
    create_test_campaign(env, &creator, 1);
    create_test_milestone(env, 0, 3000, MilestoneStatus::Unlocked);
}

// ─── Happy path: valid release (module function, with token funding) ──────────

/// Test: valid release updates milestone status to Released.
/// Calls the module function directly — auth is tested separately.
#[test]
fn test_valid_release_updates_milestone_status() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths(); // For token mint
    with_contract(&env, || {
        setup_single_milestone_campaign_with_funding(&env);
        let recipient = Address::generate(&env);
        crate::release_milestone::release_milestone(&env, 0, recipient);
        let milestone = get_milestone(&env, 0).expect("Milestone should exist");
        assert_eq!(
            milestone.status,
            MilestoneStatus::Released,
            "Milestone should transition to Released after valid release"
        );
    });
}

/// Test: valid release sets the released_amount to target_amount.
#[test]
fn test_valid_release_sets_released_amount() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        setup_single_milestone_campaign_with_funding(&env);
        let recipient = Address::generate(&env);
        crate::release_milestone::release_milestone(&env, 0, recipient);
        let milestone = get_milestone(&env, 0).expect("Milestone should exist");
        assert_eq!(
            milestone.released_amount,
            milestone.target_amount,
            "Released amount should equal target amount after release"
        );
    });
}

/// Test: final milestone releases remaining balance correctly.
#[test]
fn test_final_milestone_releases_remaining_balance() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let recipient = Address::generate(&env);
        create_test_campaign(&env, &creator, 3);
        mint_tokens_to_contract(&env);
        create_test_milestone(&env, 0, 1000, MilestoneStatus::Released);
        create_test_milestone(&env, 1, 2000, MilestoneStatus::Unlocked);
        create_test_milestone(&env, 2, 3000, MilestoneStatus::Locked);
        crate::release_milestone::release_milestone(&env, 1, recipient);
        let milestone = get_milestone(&env, 1).expect("Milestone should exist");
        assert_eq!(milestone.status, MilestoneStatus::Released);
        assert_eq!(milestone.released_amount, milestone.target_amount);
        let milestone0 = get_milestone(&env, 0).expect("Milestone should exist");
        assert_eq!(milestone0.status, MilestoneStatus::Released);
        let milestone2 = get_milestone(&env, 2).expect("Milestone should exist");
        assert_eq!(milestone2.status, MilestoneStatus::Locked);
    });
}

/// Test: releasing milestone 0 when previous is still Locked succeeds
/// because milestone 0 has no predecessor check.
#[test]
fn test_first_milestone_release_succeeds_regardless_of_previous() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        create_test_campaign(&env, &creator, 1);
        mint_tokens_to_contract(&env);
        create_test_milestone(&env, 0, 3000, MilestoneStatus::Unlocked);
        let recipient = Address::generate(&env);
        crate::release_milestone::release_milestone(&env, 0, recipient);
        let milestone = get_milestone(&env, 0).expect("Milestone should exist");
        assert_eq!(milestone.status, MilestoneStatus::Released);
    });
}

/// Test: releasing milestones in order succeeds.
#[test]
fn test_sequential_milestone_release_succeeds() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        create_test_campaign(&env, &creator, 3);
        mint_tokens_to_contract(&env);
        create_test_milestone(&env, 0, 1000, MilestoneStatus::Unlocked);
        create_test_milestone(&env, 1, 2000, MilestoneStatus::Unlocked);
        create_test_milestone(&env, 2, 3000, MilestoneStatus::Unlocked);
        let recipient = Address::generate(&env);
        crate::release_milestone::release_milestone(&env, 0, recipient.clone());
        let m0 = get_milestone(&env, 0).expect("Milestone should exist");
        assert_eq!(m0.status, MilestoneStatus::Released);
        crate::release_milestone::release_milestone(&env, 1, recipient.clone());
        let m1 = get_milestone(&env, 1).expect("Milestone should exist");
        assert_eq!(m1.status, MilestoneStatus::Released);
        crate::release_milestone::release_milestone(&env, 2, recipient);
        let m2 = get_milestone(&env, 2).expect("Milestone should exist");
        assert_eq!(m2.status, MilestoneStatus::Released);
    });
}

// ─── Auth rejection: non-creator release panics (via Client) ──────────────────

/// Test: calling release_milestone without mock_all_auths panics (auth rejected).
/// Uses the Client to test the full `#[contractimpl]` wrapper + auth path.
/// Sets up campaign WITHOUT token minting since auth fails before token ops.
#[test]
#[should_panic(expected = "HostError")]
fn test_non_creator_release_panics() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    // No mock_all_auths — auth should be rejected
    let contract_id = env.register_contract(None, crate::CampaignContract);
    let client = CampaignContractClient::new(&env, &contract_id);
    let recipient = Address::generate(&env);

    // Set up campaign storage without token mint (auth fails before token ops)
    env.as_contract(&contract_id, || {
        setup_single_milestone_campaign_no_funding(&env);
    });

    client.release_milestone(&0u32, &recipient);
}

// ─── Error path: locked milestone release panics (module function) ────────────

/// Test: releasing a Locked milestone panics with InvalidMilestoneTransition.
/// Panics at milestone status check before reaching token transfers — no funding needed.
#[test]
#[should_panic(expected = "HostError")]
fn test_locked_milestone_release_panics() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let creator = Address::generate(&env);
        create_test_campaign(&env, &creator, 1);
        create_test_milestone(&env, 0, 3000, MilestoneStatus::Locked);
        let recipient = Address::generate(&env);
        crate::release_milestone::release_milestone(&env, 0, recipient);
    });
}

// ─── Error path: skipping milestone release panics ───────────────────────────

/// Test: releasing a milestone without the previous being Released panics.
/// Panics at predecessor check before reaching token transfers — no funding needed.
#[test]
#[should_panic(expected = "HostError")]
fn test_skipping_milestone_release_panics() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let creator = Address::generate(&env);
        create_test_campaign(&env, &creator, 3);
        create_test_milestone(&env, 0, 1000, MilestoneStatus::Unlocked);
        create_test_milestone(&env, 1, 2000, MilestoneStatus::Unlocked);
        create_test_milestone(&env, 2, 3000, MilestoneStatus::Locked);
        let recipient = Address::generate(&env);
        crate::release_milestone::release_milestone(&env, 1, recipient);
    });
}

// ─── Error path: double release panics ───────────────────────────────────────

/// Test: releasing an already-Released milestone panics.
/// First release succeeds (needs token funding), second panics.
#[test]
#[should_panic(expected = "HostError")]
fn test_double_release_panics() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths(); // For token mint (first release transfers tokens)
    with_contract(&env, || {
        setup_single_milestone_campaign_with_funding(&env);
        let recipient = Address::generate(&env);
        // First release succeeds
        crate::release_milestone::release_milestone(&env, 0, recipient.clone());
        let milestone = get_milestone(&env, 0).expect("Milestone should exist");
        assert_eq!(milestone.status, MilestoneStatus::Released);
        // Second release should panic
        crate::release_milestone::release_milestone(&env, 0, recipient);
    });
}

// ─── Error path: non-existent milestone ──────────────────────────────────────

/// Test: releasing a milestone index that doesn't exist panics.
/// Panics at MilestoneNotFound before reaching token transfers — no funding needed.
#[test]
#[should_panic(expected = "HostError")]
fn test_release_non_existent_milestone_panics() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        setup_single_milestone_campaign_no_funding(&env);
        let recipient = Address::generate(&env);
        crate::release_milestone::release_milestone(&env, 5, recipient);
    });
}

// ─── Single-asset isolation: only the primary asset is touched ────────────────

/// Test: with one accepted asset, the correct amount is transferred and the
/// contract balance decreases by exactly the release amount.
#[test]
fn test_release_with_single_asset_transfers_correct_amount() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let issuers = create_multi_asset_campaign_with_funding(&env, &creator, 1, 1, 10_000_000);
        create_test_milestone(&env, 0, 3000, MilestoneStatus::Unlocked);
        let recipient = Address::generate(&env);

        crate::release_milestone::release_milestone(&env, 0, recipient.clone());

        let token_client = soroban_sdk::token::Client::new(&env, &issuers.get(0).unwrap());
        assert_eq!(token_client.balance(&recipient), 3000);
        assert_eq!(
            token_client.balance(&env.current_contract_address()),
            10_000_000 - 3000
        );
    });
}

/// Test: with three accepted assets, only the first (primary) asset is
/// debited. The other two assets' balances must remain untouched — this is
/// the regression test for the fund-draining vulnerability where
/// `release_milestone` transferred the full release amount from every
/// accepted asset instead of just one.
#[test]
fn test_release_with_multiple_assets_only_debits_first_asset() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let funding_per_asset = 10_000_000i128;
        let issuers =
            create_multi_asset_campaign_with_funding(&env, &creator, 1, 3, funding_per_asset);
        create_test_milestone(&env, 0, 3000, MilestoneStatus::Unlocked);
        let recipient = Address::generate(&env);

        crate::release_milestone::release_milestone(&env, 0, recipient.clone());

        let milestone = get_milestone(&env, 0).expect("Milestone should exist");
        assert_eq!(milestone.status, MilestoneStatus::Released);
        assert_eq!(milestone.released_amount, milestone.target_amount);

        // Primary asset (first accepted asset) was debited by the release amount.
        let primary_client = soroban_sdk::token::Client::new(&env, &issuers.get(0).unwrap());
        assert_eq!(primary_client.balance(&recipient), 3000);
        assert_eq!(
            primary_client.balance(&env.current_contract_address()),
            funding_per_asset - 3000
        );

        // Secondary assets must remain completely untouched.
        let second_client = soroban_sdk::token::Client::new(&env, &issuers.get(1).unwrap());
        assert_eq!(
            second_client.balance(&env.current_contract_address()),
            funding_per_asset
        );
        assert_eq!(second_client.balance(&recipient), 0);

        let third_client = soroban_sdk::token::Client::new(&env, &issuers.get(2).unwrap());
        assert_eq!(
            third_client.balance(&env.current_contract_address()),
            funding_per_asset
        );
        assert_eq!(third_client.balance(&recipient), 0);
    });
}

// ─── Error path: frozen contract ─────────────────────────────────────────────

/// Test: releasing a milestone while the contract is frozen panics.
/// Panics at frozen check before reaching token transfers — no funding needed.
#[test]
#[should_panic(expected = "HostError")]
fn test_frozen_contract_release_panics() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        setup_single_milestone_campaign_no_funding(&env);
        let recipient = Address::generate(&env);
        crate::storage::set_frozen(&env, true);
        crate::release_milestone::release_milestone(&env, 0, recipient);
    });
}
