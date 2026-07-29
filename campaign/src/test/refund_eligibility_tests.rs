//! Tests for `CampaignContract::is_refund_eligible` decision logic.
//!
//! Validates all five eligibility criteria: terminal state, status rules,
//! refund window, donor record existence, and no prior claim.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, Env};

use super::with_contract;
use crate::storage::{set_campaign, set_donor, set_milestone};
use crate::types::{
    AssetInfo, CampaignData, CampaignStatus, DonorRecord, MilestoneStatus, StellarAsset,
};
use crate::CampaignContract;

/// Base ledger timestamp (1 year in seconds) so we can safely subtract
/// to simulate "past" end_times without underflow.
const BASE: u64 = 86400 * 365;

/// Helper to create a test milestone
fn create_test_milestone(env: &Env, campaign_index: u32, status: MilestoneStatus) {
    let milestone = crate::types::MilestoneData {
        index: campaign_index,
        target_amount: 1000,
        released_amount: 0,
        description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
        status,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    };
    set_milestone(env, campaign_index, &milestone);
}

/// Helper to create a test campaign
fn create_test_campaign(
    env: &Env,
    status: CampaignStatus,
    goal_amount: i128,
    end_time: u64,
) -> CampaignData {
    let creator = Address::generate(env);
    let campaign = CampaignData {
        creator: creator.clone(),
        goal_amount,
        raised_amount: 0,
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
        milestone_count: 1,
        min_donation_amount: 0,
        created_at_ledger: 0,
        created_at_time: 0,
        concluded_at_ledger: None,
        max_donations_per_donor: None,
        min_donation_interval_seconds: None,
    };
    set_campaign(env, &campaign);
    create_test_milestone(env, 0, MilestoneStatus::Locked);
    campaign
}

/// Helper to create a test donor record
fn create_test_donor(
    env: &Env,
    donor: &Address,
    total_donated: i128,
    refund_claimed: bool,
) -> DonorRecord {
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
    donor_record
}

#[test]
fn test_refund_not_eligible_campaign_active() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Active, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(!eligible, "Active campaign should not be refund-eligible");
    });
}

#[test]
fn test_refund_not_eligible_campaign_goal_reached() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::GoalReached, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            !eligible,
            "GoalReached campaign should not be refund-eligible"
        );
    });
}

#[test]
fn test_refund_eligible_campaign_cancelled() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(eligible, "Cancelled campaign should be refund-eligible");
    });
}

#[test]
fn test_refund_eligible_campaign_ended_no_milestone_released() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - 100;
        create_test_campaign(&env, CampaignStatus::Ended, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            eligible,
            "Ended campaign with no milestone released should be refund-eligible"
        );
    });
}

#[test]
fn test_refund_not_eligible_no_donor_record() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
        let donor = Address::generate(&env);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(!eligible, "Non-donor should not be refund-eligible");
    });
}

#[test]
fn test_refund_not_eligible_no_campaign() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            !eligible,
            "Should not be refund-eligible if campaign not initialized"
        );
    });
}

#[test]
fn test_refund_not_eligible_window_closed() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - (31 * 24 * 60 * 60);
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            !eligible,
            "Refund should not be eligible after 30-day window closes"
        );
    });
}

#[test]
fn test_refund_not_eligible_already_claimed() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, true);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            !eligible,
            "Donor should not be refund-eligible if already claimed"
        );
    });
}

#[test]
fn test_refund_window_edge_case_exactly_30_days() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60);
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            eligible,
            "Should be refund-eligible at exactly 30-day boundary"
        );
    });
}

#[test]
fn test_refund_window_edge_case_one_second_after_30_days() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60 + 1);
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            !eligible,
            "Should not be refund-eligible after 30-day window closes"
        );
    });
}

#[test]
fn test_refund_eligibility_all_conditions() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    with_contract(&env, || {
        let end_time = env.ledger().timestamp() + 1000;
        create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
        let donor = Address::generate(&env);
        create_test_donor(&env, &donor, 100, false);
        let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
        assert!(
            eligible,
            "Should be eligible with cancelled campaign, no claim, within window"
        );
    });
}
