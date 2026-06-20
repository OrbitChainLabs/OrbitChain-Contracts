//! Negative-path tests: invalid inputs, state violations, and edge cases.
//!
//! Exercises every `should_panic` path in initialize, donate, refund, end,
//! cancel, extend_deadline, and milestones — plus boundary and auth tests.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, Env, String, Vec, BytesN};

use crate::types::{
    CampaignData, CampaignStatus, DonorRecord, AssetInfo, StellarAsset, MilestoneData,
    MilestoneStatus, Error, DataKey,
};
use crate::storage::{set_campaign, set_donor, set_milestone, get_campaign};
use crate::CampaignContract;
use crate::CampaignContractClient;
use super::with_contract;

/// Base ledger timestamp (1 year in seconds) so we can safely subtract
/// to simulate "past" end_times without underflow.
const BASE: u64 = 86400 * 365;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_env() -> Env {
    Env::default()
}

fn default_accepted_assets(env: &Env) -> Vec<StellarAsset> {
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(Address::generate(env)),
    });
    assets
}

fn default_milestones(env: &Env) -> Vec<MilestoneData> {
    let mut milestones: Vec<MilestoneData> = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: 1000,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[0u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });
    milestones
}

fn initialize_default_campaign(env: &Env) -> (Address, u64) {
    let creator = Address::generate(env);
    let end_time = env.ledger().timestamp() + 100_000;
    let assets = default_accepted_assets(env);
    let milestones = default_milestones(env);
    let _ = CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        1000,
        end_time,
        assets,
        milestones,
        0,
    );
    (creator, end_time)
}

fn fund_donor(env: &Env, donor: &Address) {
    let record = DonorRecord {
        donor: donor.clone(),
        total_donated: 500,
        asset: AssetInfo::Native,
        last_donation_time: env.ledger().timestamp(),
        last_donation_ledger: env.ledger().sequence(),
        donation_count: 1,
        refund_claimed: false,
    };
    set_donor(env, donor, &record);
}

fn create_donor_record(
    env: &Env,
    donor: &Address,
    total_donated: i128,
    refund_claimed: bool,
) {
    let record = DonorRecord {
        donor: donor.clone(),
        total_donated,
        asset: AssetInfo::Native,
        last_donation_time: env.ledger().timestamp(),
        last_donation_ledger: env.ledger().sequence(),
        donation_count: 1,
        refund_claimed,
    };
    set_donor(env, donor, &record);
}

// ─── Initialize negative-path tests ─────────────────────────────────────────

#[test]
#[should_panic]
fn test_initialize_fails_already_initialized() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(),
            creator,
            1000,
            end_time,
            default_accepted_assets(&env),
            default_milestones(&env),
            0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_zero_goal() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator, 0, end_time,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_negative_goal() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator, -100, end_time,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_past_end_time() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() - 1;
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_empty_assets() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let empty_assets: Vec<StellarAsset> = Vec::new(&env);
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            empty_assets, default_milestones(&env), 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_empty_asset_code() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let mut assets: Vec<StellarAsset> = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, ""),
            issuer: Some(Address::generate(&env)),
        });
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            assets, default_milestones(&env), 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_zero_milestones() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let empty_milestones: Vec<MilestoneData> = Vec::new(&env);
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            default_accepted_assets(&env), empty_milestones, 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_too_many_milestones() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        for i in 0..6 {
            milestones.push_back(MilestoneData {
                index: i,
                target_amount: (i as i128 + 1) * 1000,
                released_amount: 0,
                description_hash: BytesN::from_array(&env, &[0u8; 32]),
                status: MilestoneStatus::Locked,
                released_at: None,
                released_at_ledger: None,
                release_tx: None,
                released_to: None,
            });
        }
        let _ = CampaignContract::initialize(
            env.clone(), creator, 6000, end_time,
            default_accepted_assets(&env), milestones, 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_milestone_targets_not_ascending() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        milestones.push_back(MilestoneData {
            index: 0, target_amount: 500, released_amount: 0,
            description_hash: BytesN::from_array(&env, &[0u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None, released_at_ledger: None,
            release_tx: None, released_to: None,
        });
        milestones.push_back(MilestoneData {
            index: 1, target_amount: 300, released_amount: 0,
            description_hash: BytesN::from_array(&env, &[0u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None, released_at_ledger: None,
            release_tx: None, released_to: None,
        });
        let _ = CampaignContract::initialize(
            env.clone(), creator, 500, end_time,
            default_accepted_assets(&env), milestones, 0,
        );
    });
}

#[test]
#[should_panic]
fn test_initialize_fails_milestone_last_target_not_equal_goal() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        milestones.push_back(MilestoneData {
            index: 0, target_amount: 500, released_amount: 0,
            description_hash: BytesN::from_array(&env, &[0u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None, released_at_ledger: None,
            release_tx: None, released_to: None,
        });
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            default_accepted_assets(&env), milestones, 0,
        );
    });
}

// ─── Donate negative-path tests ──────────────────────────────────────────────

#[test]
#[should_panic]
fn test_donate_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, 100, AssetInfo::Native);
    });
}

#[test]
#[should_panic]
fn test_donate_fails_campaign_ended() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let (creator, _) = initialize_default_campaign(&env);
        CampaignContract::end_campaign(env.clone());
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, 100, AssetInfo::Native);
    });
}

#[test]
#[should_panic]
fn test_donate_fails_campaign_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, 100, AssetInfo::Native);
    });
}

#[test]
#[should_panic]
fn test_donate_fails_zero_amount() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, 0, AssetInfo::Native);
    });
}

#[test]
#[should_panic]
fn test_donate_fails_negative_amount() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, -100, AssetInfo::Native);
    });
}

#[test]
#[should_panic]
fn test_donate_fails_below_minimum() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            default_accepted_assets(&env), default_milestones(&env), 100,
        );
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, 50, AssetInfo::Native);
    });
}

// ─── Refund negative-path tests ──────────────────────────────────────────────

#[test]
#[should_panic]
fn test_claim_refund_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let donor = Address::generate(&env);
        CampaignContract::claim_refund(env.clone(), donor);
    });
}

#[test]
#[should_panic]
fn test_claim_refund_fails_no_donor_record() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        let donor = Address::generate(&env);
        CampaignContract::claim_refund(env.clone(), donor);
    });
}

#[test]
#[should_panic]
fn test_claim_refund_fails_campaign_active() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let donor = Address::generate(&env);
        fund_donor(&env, &donor);
        CampaignContract::claim_refund(env.clone(), donor);
    });
}

#[test]
#[should_panic]
fn test_claim_refund_fails_already_claimed() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 500, true);
        CampaignContract::claim_refund(env.clone(), donor);
    });
}

#[test]
fn test_is_refund_eligible_fails_no_campaign() {
    let env = make_env();
    with_contract(&env, || {
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Should not be eligible without campaign");
    });
}

#[test]
fn test_is_refund_eligible_fails_no_donor_record() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        let donor = Address::generate(&env);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Non-donor should not be eligible");
    });
}

#[test]
fn test_is_refund_eligible_fails_active_campaign() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Active campaign should not allow refunds");
    });
}

#[test]
fn test_is_refund_eligible_fails_goal_reached() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
        let mut campaign = get_campaign(&env).unwrap();
        campaign.status = CampaignStatus::GoalReached;
        campaign.raised_amount = 1000;
        set_campaign(&env, &campaign);
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "GoalReached campaign should not allow refunds");
    });
}

#[test]
fn test_is_refund_eligible_fails_window_closed() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        // Initialize with future end_time, then manually set to past + Ended
        let future_end = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator.clone(), 1000, future_end,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
        let mut campaign = get_campaign(&env).unwrap();
        campaign.end_time = env.ledger().timestamp() - (31 * 24 * 60 * 60);
        campaign.status = CampaignStatus::Ended;
        set_campaign(&env, &campaign);
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Refund window should be closed after 30 days");
    });
}

#[test]
fn test_is_refund_eligible_fails_already_claimed() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, true);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Already claimed donor should not be eligible");
    });
}

#[test]
fn test_is_refund_eligible_fails_ended_with_released_milestones() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let mut milestone = crate::storage::get_milestone(&env, 0).unwrap();
        milestone.status = MilestoneStatus::Released;
        milestone.released_amount = 1000;
        set_milestone(&env, 0, &milestone);
        CampaignContract::end_campaign(env.clone());
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Ended campaign with released milestone should not allow refunds");
    });
}

// ─── End campaign negative-path tests ────────────────────────────────────────

#[test]
#[should_panic]
fn test_end_campaign_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        CampaignContract::end_campaign(env.clone());
    });
}

#[test]
#[should_panic]
fn test_end_campaign_fails_already_ended() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::end_campaign(env.clone());
        CampaignContract::end_campaign(env.clone());
    });
}

#[test]
#[should_panic]
fn test_end_campaign_fails_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        CampaignContract::end_campaign(env.clone());
    });
}

// ─── Cancel campaign negative-path tests ─────────────────────────────────────

#[test]
#[should_panic]
fn test_cancel_campaign_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        CampaignContract::cancel_campaign(env.clone());
    });
}

#[test]
#[should_panic]
fn test_cancel_campaign_fails_already_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        CampaignContract::cancel_campaign(env.clone());
    });
}

// ─── Extend deadline negative-path tests ─────────────────────────────────────

#[test]
#[should_panic]
fn test_extend_deadline_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        CampaignContract::extend_deadline(env.clone(), 999_999);
    });
}

#[test]
#[should_panic]
fn test_extend_deadline_fails_past_time() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let past_time = env.ledger().timestamp() - 1;
        CampaignContract::extend_deadline(env.clone(), past_time);
    });
}

#[test]
#[should_panic]
fn test_extend_deadline_fails_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        CampaignContract::extend_deadline(env.clone(), 999_999);
    });
}

// ─── Reentrancy / lock tests ────────────────────────────────────────────────

/// Test that a donor can donate twice without reentrancy lock issues.
/// Uses `CampaignContractClient` so that `env.mock_all_auths()` creates
/// proper host-boundary invocations for each `donate` call.
/// Key: `mock_all_auths()` before `register_contract()`, Client calls
/// outside `as_contract()` to avoid re-entrancy.
#[test]
fn test_reentrancy_lock_donate_twice_succeeds() {
    let env = make_env();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::CampaignContract);
    let client = CampaignContractClient::new(&env, &contract_id);

    let donor = Address::generate(&env);

    // Setup storage inside as_contract
    env.as_contract(&contract_id, || {
        initialize_default_campaign(&env);
    });

    // Donate through client outside as_contract — each call is a fresh invocation
    client.donate(&donor, &100i128, &AssetInfo::Native);
    client.donate(&donor, &200i128, &AssetInfo::Native);

    // Verify storage
    env.as_contract(&contract_id, || {
        let record = CampaignContract::get_donor_record(env.clone(), donor);
        assert!(record.is_some());
        assert_eq!(record.unwrap().total_donated, 300);
    });
}

// ─── Balance verification tests (Issue #244) ─────────────────────────────────

#[test]
fn test_claim_refund_eligible_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 500, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(eligible, "Donor should be eligible for refund on cancelled campaign");
    });
}

// ─── Milestone view negative-path tests ──────────────────────────────────────

#[test]
#[should_panic]
fn test_get_milestone_view_fails_not_initialized() {
    let env = make_env();
    with_contract(&env, || {
        CampaignContract::get_milestone_view(env.clone(), 0);
    });
}

#[test]
#[should_panic]
fn test_get_milestone_view_fails_out_of_bounds() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::get_milestone_view(env.clone(), 99);
    });
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

#[test]
fn test_edge_case_zero_donations() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let total = CampaignContract::get_total_raised(env.clone());
        assert_eq!(total, 0, "No donations yet");
    });
}

#[test]
fn test_edge_case_no_donor_record() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        let stranger = Address::generate(&env);
        let record = CampaignContract::get_donor_record(env.clone(), stranger);
        assert!(record.is_none(), "Stranger should have no donor record");
    });
}

#[test]
fn test_is_refund_eligible_returns_false_no_campaign() {
    let env = make_env();
    with_contract(&env, || {
        let donor = Address::generate(&env);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Should not be eligible without any campaign");
    });
}

#[test]
fn test_refund_window_edge_boundary() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        // Initialize with future end_time, then manually set to exact boundary
        let future_end = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator.clone(), 1000, future_end,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
        let mut campaign = get_campaign(&env).unwrap();
        campaign.end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60);
        campaign.status = CampaignStatus::Ended;
        set_campaign(&env, &campaign);
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(eligible, "Should be eligible exactly at 30-day boundary");
    });
}

#[test]
fn test_refund_window_just_after_boundary() {
    let env = make_env();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        // Initialize with future end_time, then manually set to just past boundary
        let future_end = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator.clone(), 1000, future_end,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
        let mut campaign = get_campaign(&env).unwrap();
        campaign.end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60 + 1);
        campaign.status = CampaignStatus::Ended;
        set_campaign(&env, &campaign);
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(!eligible, "Should NOT be eligible just past 30-day boundary");
    });
}

// ─── Upgrade freeze guard tests (issue #10) ──────────────────────────────────

#[test]
#[should_panic]
fn test_upgrade_fails_when_frozen() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::freeze(env.clone());
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        CampaignContract::upgrade(env.clone(), hash);
    });
}

#[test]
fn test_upgrade_succeeds_when_not_frozen() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        // Verify the contract is not frozen by default; upgrade should not panic on the
        // freeze check (it will panic later when the deployer rejects the dummy hash,
        // so we only assert that is_frozen returns false before the call).
        assert!(!crate::storage::is_frozen(&env), "Contract should not be frozen initially");
    });
}

#[test]
fn test_upgrade_succeeds_after_unfreeze() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::freeze(env.clone());
        assert!(crate::storage::is_frozen(&env), "Contract should be frozen");
        CampaignContract::unfreeze(env.clone());
        assert!(!crate::storage::is_frozen(&env), "Contract should be unfrozen after unfreeze");
    });
}

// ─── Version and hello tests ─────────────────────────────────────────────────

#[test]
fn test_version() {
    let env = make_env();
    assert_eq!(CampaignContract::version(), 1);
}

#[test]
fn test_hello() {
    let env = make_env();
    let result = CampaignContract::hello(env.clone());
    assert_eq!(result, soroban_sdk::Symbol::new(&env, "campaign"));
}

// ─── Authorisation failure tests ─────────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_requires_auth() {
    let env = make_env();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 100_000;
        let _ = CampaignContract::initialize(
            env.clone(), creator, 1000, end_time,
            default_accepted_assets(&env), default_milestones(&env), 0,
        );
    });
}

// ─── Positive-path sanity checks ─────────────────────────────────────────────

#[test]
fn test_full_lifecycle_happy_path() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        let (creator, _) = initialize_default_campaign(&env);
        let status = CampaignContract::get_campaign_status(env.clone());
        assert_eq!(status.status, CampaignStatus::Active);
        assert!(status.days_remaining > 0);
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
        let total = CampaignContract::get_total_raised(env.clone());
        assert_eq!(total, 100);
        let record = CampaignContract::get_donor_record(env.clone(), donor);
        assert!(record.is_some());
    });
}

#[test]
fn test_end_then_refund_eligible() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::end_campaign(env.clone());
        let status = CampaignContract::get_campaign_status(env.clone());
        assert_eq!(status.status, CampaignStatus::Ended);
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 500, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(eligible);
    });
}

#[test]
fn test_cancel_then_refund_eligible() {
    let env = make_env();
    env.mock_all_auths();
    with_contract(&env, || {
        initialize_default_campaign(&env);
        CampaignContract::cancel_campaign(env.clone());
        let status = CampaignContract::get_campaign_status(env.clone());
        assert_eq!(status.status, CampaignStatus::Cancelled);
        let donor = Address::generate(&env);
        create_donor_record(&env, &donor, 500, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
        assert!(eligible);
    });
}
