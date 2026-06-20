//! Tests for `CampaignContract::claim_refund` and refund eligibility edge cases.
//!
//! Validates the full refund decision matrix: campaign status, milestone state,
//! refund window boundaries, and already-claimed protection.

#![cfg(test)]

use core::ops::Add;

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env, Vec, log, vec};

use crate::types::{AssetInfo, CampaignData, CampaignStatus, DonorRecord, MilestoneData, MilestoneStatus, StellarAsset};
use crate::storage::{set_campaign, set_donor, set_milestone};
use crate::{CampaignContract, CampaignContractClient};
use super::with_contract;

/// Base ledger timestamp (1 year in seconds) used so we can safely subtract
/// from it to simulate "past" end_times without underflow.
const BASE: u64 = 86400 * 365;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_env() -> Env {
    Env::default()
}

/// Creates a test campaign with the given status and returns its data.
fn create_test_campaign(
    env: &Env,
    status: CampaignStatus,
    goal_amount: i128,
    end_time: u64,
    milestone_count: u32,
) -> CampaignData {
    let creator = Address::generate(env);
    let campaign = CampaignData {
        creator: creator.clone(),
        goal_amount,
        raised_amount: if matches!(status, CampaignStatus::Cancelled | CampaignStatus::Ended) { 1000 } else { 0 },
        end_time,
        status,
        accepted_assets: {
            let mut assets = soroban_sdk::Vec::new(env);
            assets.push_back(StellarAsset {
                asset_code: soroban_sdk::String::from_str(env, "XLM"),
                issuer: Some(Address::generate(env)),
            });
            assets
        },
        milestone_count,
        min_donation_amount: 0,
        created_at_ledger: 0,
        created_at_time: 0,
        concluded_at_ledger: None,
    };
    set_campaign(env, &campaign);
    campaign
}

/// Creates a milestone with the given index and status.
fn create_test_milestone(
    env: &Env,
    index: u32,
    target_amount: i128,
    status: MilestoneStatus,
) {
    let milestone = crate::types::MilestoneData {
        index,
        target_amount,
        released_amount: if status == MilestoneStatus::Released { target_amount } else { 0 },
        description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
        status,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    };
    set_milestone(env, index, &milestone);
}

/// Creates a donor record for testing.
fn create_test_donor(
    env: &Env,
    donor: &Address,
    total_donated: i128,
    refund_claimed: bool,
) {
    let donor_record = DonorRecord {
        donor: donor.clone(),
        total_donated,
        asset: AssetInfo::Native,
        last_donation_time: 0,
        last_donation_ledger: 0,
        donation_count: 1,
        refund_claimed,
    };
    set_donor(env, donor, &donor_record);
}

// ─── Error path tests ────────────────────────────────────────────────────────

/// Claiming a refund when no campaign has been initialized should panic.
#[test]
#[should_panic(expected = "HostError")]
fn test_claim_refund_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let donor = Address::generate(&env);
        CampaignContract::claim_refund(env.clone(), donor.clone());
    });
}

/// A donor who has never donated should not be able to claim a refund.
#[test]
#[should_panic(expected = "HostError")]
fn test_claim_refund_no_donor_record() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time, 1);
        let donor = Address::generate(&env);
        CampaignContract::claim_refund(env.clone(), donor.clone());
    });
}

/// Refunds should not be allowed while the campaign is Active.
#[test]
#[should_panic(expected = "HostError")]
fn test_claim_refund_active_campaign() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Active, 1000, end_time, 1);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        CampaignContract::claim_refund(env.clone(), donor.clone());
    });
}

/// Refunds should not be allowed while the campaign is in GoalReached status.
#[test]
#[should_panic(expected = "HostError")]
fn test_claim_refund_goal_reached_campaign() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::GoalReached, 1000, end_time, 1);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        CampaignContract::claim_refund(env.clone(), donor.clone());
    });
}

/// Refunds should not be allowed on an Ended campaign when a milestone has already been released.
#[test]
#[should_panic(expected = "HostError")]
fn test_claim_refund_ended_with_milestone_released() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - 100;
        create_test_campaign(&env, CampaignStatus::Ended, 1000, end_time, 1);
        create_test_milestone(&env, 0, 1000, MilestoneStatus::Released);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        CampaignContract::claim_refund(env.clone(), donor.clone());
    });
}

/// Refunds should not be allowed if the 30-day refund window has closed.
#[test]
#[should_panic(expected = "HostError")]
fn test_claim_refund_window_closed() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - (31 * 24 * 60 * 60);
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time, 1);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        CampaignContract::claim_refund(env.clone(), donor.clone());
    });
}

/// A donor who has already claimed a refund should not be able to claim again.
#[test]
#[should_panic(expected = "HostError")]
fn test_claim_refund_already_claimed() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time, 1);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, true);
        CampaignContract::claim_refund(env.clone(), donor.clone());
    });
}

/// Exactly at the 30-day window boundary should still allow refunds.
#[test]
fn test_claim_refund_exactly_at_window_boundary() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60);
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time, 1);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(eligible, "Should be refund-eligible at exactly 30-day boundary");
    });
}

/// One second past the 30-day window should deny refunds.
#[test]
fn test_claim_refund_one_second_past_window() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60 + 1);
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time, 1);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(!eligible, "Should NOT be refund-eligible past 30-day window");
    });
}

/// A donor with zero donation should not be eligible for refund (no donor record = not a donor).
#[test]
fn test_claim_refund_no_donor_eligibility() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time, 1);
        let non_donor = Address::generate(&env);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), non_donor.clone());
        assert!(!eligible, "Non-donor should not be refund-eligible");
    });
}

/// On an Ended campaign with no milestones released, the refund should be eligible.
#[test]
fn test_claim_refund_ended_no_milestones_eligibility() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - 100;
        create_test_campaign(&env, CampaignStatus::Ended, 1000, end_time, 1);
        create_test_milestone(&env, 0, 1000, MilestoneStatus::Locked);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(eligible, "Ended campaign with no released milestones should allow refunds");
    });
}

/// On an Ended campaign with a released milestone, refund should NOT be eligible.
#[test]
fn test_claim_refund_ended_with_released_milestone_eligibility() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - 100;
        create_test_campaign(&env, CampaignStatus::Ended, 1000, end_time, 1);
        create_test_milestone(&env, 0, 1000, MilestoneStatus::Released);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(!eligible, "Ended campaign with released milestones should NOT allow refunds");
    });
}


fn setup<'a>() -> (Env, CampaignContractClient<'a>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CampaignContract, ());
    let client = CampaignContractClient::new(&env, &contract_id);

    let creator = Address::generate(&env);
    (env, client, creator)
}

fn create_test_milestone_data(
    env: &Env,
    index: u32,
    target_amount: i128,
    status: MilestoneStatus,
) -> Vec<MilestoneData>{
    let milestone = crate::types::MilestoneData {
        index,
        target_amount,
        released_amount: if status == MilestoneStatus::Released { target_amount } else { 0 },
        description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
        status,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    };
    vec![&env, milestone]
}

fn token_asset<'a>(env: &Env) -> (StellarAssetClient<'a>, Address, TokenClient) {
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_address = sac.address();
    let token = TokenClient::new(&env, &token_address);
    let token_sac = StellarAssetClient::new(&env, &token_address);

    (token_sac, token_address, token)
}

#[test]
fn test_claim_refund_ended_donor_100() {
    let (env, client, creator) = setup();
    env.mock_all_auths();
    env.ledger().set_timestamp(BASE);

    let (token_sac, token_address, token) = token_asset(&env);

    let donor = Address::generate(&env);
    let donor2 = Address::generate(&env);
    token_sac.mint(&donor, &100);
    token_sac.mint(&donor2, &1_000_000);

    let goal_amount = 999_000;
    let end_time = env.ledger().timestamp() + 1000;
    let mut accepted_assets = soroban_sdk::Vec::new(&env);
    accepted_assets.push_back(StellarAsset {
        asset_code: soroban_sdk::String::from_str(&env, "TST"),
        issuer: Some(token_address.clone()),
    });
    let milestones = create_test_milestone_data(&env, 0, 999_000, MilestoneStatus::Locked);
    let min_donation_amount = 0;
    let contract_address = &client.address;

    client.initialize(&creator, &goal_amount, &end_time, &accepted_assets, &milestones, &min_donation_amount);

    client.donate(&donor, &100, &AssetInfo::Stellar(token_address.clone()));
    token.transfer(&donor, contract_address, &100);
    client.donate(&donor2, &999_900, &AssetInfo::Stellar(token_address.clone()));
    token.transfer(&donor2, contract_address, &999_900);


    let recipient = Address::generate(&env);
    client.release_milestone(&0, &recipient);

    client.cancel_campaign();

    let is_refund_eligible = client.is_refund_eligible(&donor);
    assert!(is_refund_eligible);

    client.claim_refund(&donor);
    client.claim_refund(&donor2);

    let donor_balance = token.balance(&donor);
    assert_eq!(donor_balance, 1);

    let donor2_balance = token.balance(&donor2);
    assert_eq!(donor2_balance, 1099);
    
    let contract_balance = token.balance(&contract_address);
    assert_eq!(contract_balance, 0);
}

#[test]
fn test_claim_refund_ended_donor_1() {
    let (env, client, creator) = setup();
    env.mock_all_auths();
    env.ledger().set_timestamp(BASE);

    let (token_sac, token_address, token) = token_asset(&env);

    let donor = Address::generate(&env);
    let donor2 = Address::generate(&env);
    token_sac.mint(&donor, &100);
    token_sac.mint(&donor2, &1_000_000);

    let goal_amount = 9999;
    let end_time = env.ledger().timestamp() + 1000;
    let mut accepted_assets = soroban_sdk::Vec::new(&env);
    accepted_assets.push_back(StellarAsset {
        asset_code: soroban_sdk::String::from_str(&env, "TST"),
        issuer: Some(token_address.clone()),
    });
    let milestones = create_test_milestone_data(&env, 0, 9999, MilestoneStatus::Locked);
    let min_donation_amount = 0;
    let contract_address = &client.address;

    client.initialize(&creator, &goal_amount, &end_time, &accepted_assets, &milestones, &min_donation_amount);

    client.donate(&donor, &1, &AssetInfo::Stellar(token_address.clone()));
    token.transfer(&donor, contract_address, &1);
    client.donate(&donor2, &9999, &AssetInfo::Stellar(token_address.clone()));
    token.transfer(&donor2, contract_address, &9999);


    let recipient = Address::generate(&env);
    client.release_milestone(&0, &recipient);

    client.cancel_campaign();

    let is_refund_eligible = client.is_refund_eligible(&donor);
    assert!(is_refund_eligible);

    client.claim_refund(&donor);
    let donor_balance = token.balance(&donor);
    assert_eq!(donor_balance, 100);

    let donor2_balance = token.balance(&donor2);
    assert_eq!(donor2_balance, 990001);

    let contract_balance = token.balance(&contract_address);
    assert_eq!(contract_balance, 0);
}

#[test]
fn test_claim_refund_ended_full_refund() {
    let (env, client, creator) = setup();
    env.mock_all_auths();
    env.ledger().set_timestamp(BASE);

    let (token_sac, token_address, token) = token_asset(&env);

    let donor = Address::generate(&env);
    let donor2 = Address::generate(&env);
    token_sac.mint(&donor, &10000);
    token_sac.mint(&donor2, &1_000_000);

    let goal_amount = 1000;
    let end_time = env.ledger().timestamp() + 1000;
    let mut accepted_assets = soroban_sdk::Vec::new(&env);
    accepted_assets.push_back(StellarAsset {
        asset_code: soroban_sdk::String::from_str(&env, "TST"),
        issuer: Some(token_address.clone()),
    });
    let milestones = create_test_milestone_data(&env, 0, 1000, MilestoneStatus::Locked);
    let min_donation_amount = 0;
    let contract_address = &client.address;

    client.initialize(&creator, &goal_amount, &end_time, &accepted_assets, &milestones, &min_donation_amount);

    client.donate(&donor, &1500, &AssetInfo::Stellar(token_address.clone()));
    token.transfer(&donor, contract_address, &1500);


    let recipient = Address::generate(&env);
    client.release_milestone(&0, &recipient);

    client.cancel_campaign();

    let is_refund_eligible = client.is_refund_eligible(&donor);
    assert!(is_refund_eligible);

    client.claim_refund(&donor);
    let donor_balance = token.balance(&donor);
    assert_eq!(donor_balance, 9000);

    let contract_balance = token.balance(&contract_address);
    assert_eq!(contract_balance, 0);
}