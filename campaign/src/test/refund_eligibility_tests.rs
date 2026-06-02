#![cfg(test)]

use soroban_sdk::testutils::Address as AddressTestUtils;
use soroban_sdk::{Address, Env};

use crate::types::{CampaignStatus, CampaignData, DonorRecord, AssetInfo, StellarAsset, MilestoneStatus};
use crate::storage::{set_campaign, set_donor, get_milestone, set_milestone};
use crate::CampaignContract;

/// Helper to create a test milestone
fn create_test_milestone(
    env: &Env,
    campaign_index: u32,
    status: MilestoneStatus,
) {
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
    };
    set_campaign(env, &campaign);
    // Create the milestone for this campaign
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
    let end_time = env.ledger().timestamp() + 1000;
    create_test_campaign(&env, CampaignStatus::Active, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Active campaigns don't allow refunds
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(!eligible, "Active campaign should not be refund-eligible");
}

#[test]
fn test_refund_not_eligible_campaign_goal_reached() {
    let env = Env::default();
    let end_time = env.ledger().timestamp() + 1000;
    create_test_campaign(&env, CampaignStatus::GoalReached, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // GoalReached campaigns don't allow refunds
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(!eligible, "GoalReached campaign should not be refund-eligible");
}

#[test]
fn test_refund_eligible_campaign_cancelled() {
    let env = Env::default();
    let end_time = env.ledger().timestamp() + 1000;
    create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Cancelled campaigns always allow refunds (within window)
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(eligible, "Cancelled campaign should be refund-eligible");
}

#[test]
fn test_refund_eligible_campaign_ended_no_milestone_released() {
    let env = Env::default();
    let end_time = env.ledger().timestamp() - 100; // Campaign ended
    create_test_campaign(&env, CampaignStatus::Ended, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Ended campaigns allow refunds only if no milestone released
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(eligible, "Ended campaign with no milestone released should be refund-eligible");
}

#[test]
fn test_refund_not_eligible_no_donor_record() {
    let env = Env::default();
    let end_time = env.ledger().timestamp() + 1000;
    create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
    
    let donor = Address::generate(&env);
    // Don't create a donor record

    // Non-donors can't get refunds
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(!eligible, "Non-donor should not be refund-eligible");
}

#[test]
fn test_refund_not_eligible_no_campaign() {
    let env = Env::default();
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Can't refund if campaign doesn't exist
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(!eligible, "Should not be refund-eligible if campaign not initialized");
}

#[test]
fn test_refund_not_eligible_window_closed() {
    let env = Env::default();
    // Campaign ended more than 30 days ago
    let end_time = env.ledger().timestamp() - (31 * 24 * 60 * 60);
    create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Window must be within 30 days
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(!eligible, "Refund should not be eligible after 30-day window closes");
}

#[test]
fn test_refund_not_eligible_already_claimed() {
    let env = Env::default();
    let end_time = env.ledger().timestamp() + 1000;
    create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, true); // Already claimed

    // Can't claim refund twice
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(!eligible, "Donor should not be refund-eligible if already claimed");
}

#[test]
fn test_refund_window_edge_case_exactly_30_days() {
    let env = Env::default();
    // Campaign ended exactly 30 days ago
    let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60);
    create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Should be eligible at exactly 30 days
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(eligible, "Should be refund-eligible at exactly 30-day boundary");
}

#[test]
fn test_refund_window_edge_case_one_second_after_30_days() {
    let env = Env::default();
    // Campaign ended 30 days + 1 second ago
    let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60 + 1);
    create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
    
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Should NOT be eligible past 30 days
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(!eligible, "Should not be refund-eligible after 30-day window closes");
}

#[test]
fn test_refund_eligibility_all_conditions() {
    // Test that all conditions must be true for eligibility
    let env = Env::default();
    let end_time = env.ledger().timestamp() + 1000;
    
    // Start with a cancelled campaign (eligible)
    create_test_campaign(&env, CampaignStatus::Cancelled, 1000, end_time);
    let donor = Address::generate(&env);
    create_test_donor(&env, &donor, 100, false);

    // Should be eligible
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor.clone());
    assert!(eligible, "Should be eligible with cancelled campaign, no claim, within window");
}
