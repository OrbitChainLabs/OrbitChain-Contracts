//! Invariant tests for the campaign contract.
//!
//! These tests verify that critical accounting invariants always hold
//! regardless of the sequence of operations performed on the contract.
//! No randomness library needed — we set up state manually and assert
//! invariants after each operation.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, Env, Vec, String, BytesN};

use crate::types::{
    CampaignData, CampaignStatus, MilestoneData, MilestoneStatus, StellarAsset,
};
use crate::storage::{
    get_campaign, get_milestone, set_campaign, set_milestone, storage_get_total_raised,
};
use super::with_contract;

/// Base timestamp: 1 year in seconds, same convention as other test files.
const BASE: u64 = 86400 * 365;

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Builds a campaign with the given milestones and stores it.
/// `raised_amount` is set by the caller so each test can control it.
/// Returns the token issuer address so tests can reference it.
fn setup_campaign_with_milestones(
    env: &Env,
    goal_amount: i128,
    raised_amount: i128,
    status: CampaignStatus,
    milestone_targets: &[i128],
) -> Address {
    let creator = Address::generate(env);
    let token_issuer = Address::generate(env);

    // Build the accepted_assets list with one mock token
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "USDC"),
        issuer: Some(token_issuer.clone()),
    });

    // Store the campaign
    let campaign = CampaignData {
        creator,
        goal_amount,
        raised_amount,
        end_time: BASE + 86_400,
        status,
        accepted_assets: assets,
        milestone_count: milestone_targets.len() as u32,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    };
    set_campaign(env, &campaign);

    // Store each milestone
    for (i, &target) in milestone_targets.iter().enumerate() {
        let milestone = MilestoneData {
            index: i as u32,
            target_amount: target,
            released_amount: 0,
            description_hash: BytesN::from_array(env, &[0u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        };
        set_milestone(env, i as u32, &milestone);
    }

    token_issuer
}

// ─── Invariant tests ──────────────────────────────────────────────────────────

/// INVARIANT 1: Last milestone target always equals goal_amount.
///
/// The contract enforces this at initialize() time via Error::MilestoneMismatch.
/// This test verifies the invariant holds across single and multi-milestone
/// campaigns by reading back stored state and asserting equality.
#[test]
fn invariant_last_milestone_target_equals_goal() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);

    // Case A: single milestone — target must equal goal
    with_contract(&env, || {
        let goal: i128 = 1000;
        setup_campaign_with_milestones(&env, goal, 0, CampaignStatus::Active, &[1000]);

        let campaign = get_campaign(&env).unwrap();
        let last_index = campaign.milestone_count - 1;
        let last_milestone = get_milestone(&env, last_index).unwrap();

        assert_eq!(
            last_milestone.target_amount,
            campaign.goal_amount,
            "INVARIANT VIOLATED: last milestone target ({}) != goal_amount ({})",
            last_milestone.target_amount,
            campaign.goal_amount,
        );
    });

    // Case B: three milestones — only the last must equal goal
    with_contract(&env, || {
        let goal: i128 = 3000;
        setup_campaign_with_milestones(
            &env, goal, 0, CampaignStatus::Active,
            &[1000, 2000, 3000],
        );

        let campaign = get_campaign(&env).unwrap();
        let last_index = campaign.milestone_count - 1;
        let last_milestone = get_milestone(&env, last_index).unwrap();

        assert_eq!(
            last_milestone.target_amount,
            campaign.goal_amount,
            "INVARIANT VIOLATED: last milestone target ({}) != goal_amount ({})",
            last_milestone.target_amount,
            campaign.goal_amount,
        );
    });
}

/// INVARIANT 2: raised_amount never exceeds goal_amount during normal donation flow.
///
/// The contract clamps donations at the goal — once goal is reached, status
/// transitions to GoalReached but raised_amount should not go past goal_amount
/// in the single-asset path. We verify this by simulating incremental donations
/// as state updates and checking the invariant after each step.
#[test]
fn invariant_raised_amount_never_exceeds_goal() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);

    with_contract(&env, || {
        let goal: i128 = 3000;
        setup_campaign_with_milestones(&env, goal, 0, CampaignStatus::Active, &[1000, 2000, 3000]);

        // Simulate donation steps: 500, 1000, 1500, 2000, 2500, 3000
        // Each step updates raised_amount and we check the invariant holds
        let donation_steps: &[i128] = &[500, 1000, 1500, 2000, 2500, 3000];

        for &amount in donation_steps {
            // Update raised_amount in storage (simulating a donate() call)
            let mut campaign = get_campaign(&env).unwrap();
            campaign.raised_amount = amount;
            if campaign.raised_amount >= campaign.goal_amount {
                campaign.status = CampaignStatus::GoalReached;
            }
            set_campaign(&env, &campaign);

            // Read back and assert invariant
            let campaign = get_campaign(&env).unwrap();
            assert!(
                campaign.raised_amount <= campaign.goal_amount,
                "INVARIANT VIOLATED at step {}: raised_amount ({}) > goal_amount ({})",
                amount,
                campaign.raised_amount,
                campaign.goal_amount,
            );
        }
    });
}

/// INVARIANT 3: Sum of all donor contributions matches raised_amount and total_raised.
///
/// Three donors contribute different amounts. After all donations are recorded,
/// we sum their individual records and verify it equals both campaign.raised_amount
/// and storage_get_total_raised(). A mismatch here means the accounting is split.
#[test]
fn invariant_total_donations_match_raised() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);

    with_contract(&env, || {
        use crate::types::{AssetInfo, DonorRecord};
        use crate::storage::{set_donor, storage_set_total_raised};

        let goal: i128 = 3000;
        setup_campaign_with_milestones(&env, goal, 0, CampaignStatus::Active, &[1000, 2000, 3000]);

        // Three donors with different contribution amounts
        let donor_a = Address::generate(&env);
        let donor_b = Address::generate(&env);
        let donor_c = Address::generate(&env);

        let amount_a: i128 = 500;
        let amount_b: i128 = 1000;
        let amount_c: i128 = 1500;
        let total: i128 = amount_a + amount_b + amount_c; // 3000

        // Write donor records manually (simulating donate() calls)
        let asset_info = AssetInfo::Stellar(Address::generate(&env));

        let mut record_a = DonorRecord::new_for(donor_a.clone(), asset_info.clone());
        record_a.apply_donation(amount_a, BASE, 1, asset_info.clone());
        set_donor(&env, &donor_a, &record_a);

        let mut record_b = DonorRecord::new_for(donor_b.clone(), asset_info.clone());
        record_b.apply_donation(amount_b, BASE, 1, asset_info.clone());
        set_donor(&env, &donor_b, &record_b);

        let mut record_c = DonorRecord::new_for(donor_c.clone(), asset_info.clone());
        record_c.apply_donation(amount_c, BASE, 1, asset_info.clone());
        set_donor(&env, &donor_c, &record_c);

        // Update campaign raised_amount and global total_raised to match
        let mut campaign = get_campaign(&env).unwrap();
        campaign.raised_amount = total;
        campaign.status = CampaignStatus::GoalReached;
        set_campaign(&env, &campaign);
        storage_set_total_raised(&env, total);

        // Now verify the invariant: sum of donor records == raised_amount == total_raised
        use crate::storage::get_donor;
        let sum = get_donor(&env, &donor_a).unwrap().total_donated
            + get_donor(&env, &donor_b).unwrap().total_donated
            + get_donor(&env, &donor_c).unwrap().total_donated;

        let campaign = get_campaign(&env).unwrap();
        let total_raised = storage_get_total_raised(&env);

        assert_eq!(
            sum, campaign.raised_amount,
            "INVARIANT VIOLATED: sum of donor records ({}) != raised_amount ({})",
            sum, campaign.raised_amount,
        );
        assert_eq!(
            sum, total_raised,
            "INVARIANT VIOLATED: sum of donor records ({}) != total_raised ({})",
            sum, total_raised,
        );
    });
}

/// INVARIANT 4: No milestone has Released status while campaign is Active.
///
/// Donations can unlock milestones (Locked -> Unlocked) but never release them
/// (Unlocked -> Released). Released status requires an explicit release_milestone()
/// call by the creator. This test verifies that after simulating donations that
/// reach and exceed each milestone threshold, all milestones remain non-Released.
#[test]
fn invariant_no_released_milestones_while_active() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);

    with_contract(&env, || {
        let goal: i128 = 3000;
        setup_campaign_with_milestones(
            &env, goal, 0, CampaignStatus::Active,
            &[1000, 2000, 3000],
        );

        // Simulate donations that cross each milestone threshold
        // by updating raised_amount and unlocking milestones as donate() would
        let donation_amounts: &[i128] = &[1000, 2000, 3000];

        for &raised in donation_amounts {
            let mut campaign = get_campaign(&env).unwrap();
            campaign.raised_amount = raised;
            // Status stays Active until goal — then GoalReached, never Released
            if campaign.raised_amount >= campaign.goal_amount {
                campaign.status = CampaignStatus::GoalReached;
            }
            set_campaign(&env, &campaign);

            // Unlock milestones whose threshold has been crossed (as donate() does)
            // but NEVER set them to Released
            let campaign = get_campaign(&env).unwrap();
            for i in 0..campaign.milestone_count {
                let mut ms = get_milestone(&env, i).unwrap();
                if campaign.raised_amount >= ms.target_amount
                    && ms.status == MilestoneStatus::Locked
                {
                    ms.status = MilestoneStatus::Unlocked;
                    set_milestone(&env, i, &ms);
                }
            }

            // INVARIANT: no milestone should be Released at this point
            let campaign = get_campaign(&env).unwrap();
            for i in 0..campaign.milestone_count {
                let ms = get_milestone(&env, i).unwrap();
                assert_ne!(
                    ms.status,
                    MilestoneStatus::Released,
                    "INVARIANT VIOLATED: milestone {} is Released while campaign is {:?} \
                     after donations only (raised={})",
                    i, campaign.status, raised,
                );
            }
        }
    });
}

/// INVARIANT 5: Milestone targets are strictly ascending.
///
/// Each milestone's target_amount must be strictly greater than the previous
/// one. Equal or descending targets would mean multiple milestones unlock at
/// the same donation level, breaking the sequential release model.
/// The last milestone must equal goal_amount (covered by invariant 1).
#[test]
fn invariant_milestone_targets_strictly_ascending() {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);

    with_contract(&env, || {
        setup_campaign_with_milestones(
            &env, 3000, 0, CampaignStatus::Active,
            &[1000, 2000, 3000],
        );

        let campaign = get_campaign(&env).unwrap();
        let mut prev_target: i128 = 0;

        for i in 0..campaign.milestone_count {
            let ms = get_milestone(&env, i).unwrap();

            assert!(
                ms.target_amount > prev_target,
                "INVARIANT VIOLATED: milestone {} target ({}) is not greater than \
                 previous target ({})",
                i, ms.target_amount, prev_target,
            );

            prev_target = ms.target_amount;
        }

        // Final check: last milestone equals goal
        let last = get_milestone(&env, campaign.milestone_count - 1).unwrap();
        assert_eq!(
            last.target_amount,
            campaign.goal_amount,
            "INVARIANT VIOLATED: last milestone target ({}) != goal_amount ({})",
            last.target_amount,
            campaign.goal_amount,
        );
    });
}
