#![cfg(test)]

use soroban_sdk::testutils::Address as AddressTestUtils;
use soroban_sdk::{Address, Env, Vec, String, BytesN};
use soroban_sdk::token::StellarAssetClient;

use crate::types::{CampaignStatus, MilestoneStatus, MilestoneData, StellarAsset, AssetInfo, CampaignData};
use crate::storage::{get_campaign, get_milestone, set_campaign, set_milestone, storage_set_total_raised, storage_set_asset_raised};
use crate::CampaignContract;
use super::with_contract;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Builds a minimal valid campaign setup and returns (creator, assets, milestones).
fn setup_basic_campaign(env: &Env) -> (Address, Vec<StellarAsset>, Vec<MilestoneData>) {
    let creator = Address::generate(env);
    
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(Address::generate(env)),
    });

    let mut milestones: Vec<MilestoneData> = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: 1000,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[1u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });

    (creator, assets, milestones)
}

// ─── Invariant 1: last milestone target must equal goal ───────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #20)")]
fn invariant_last_milestone_target_equals_goal() {
    for &goal in &[500i128, 1000, 5000] {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let creator = Address::generate(&env);
            let mut assets: Vec<StellarAsset> = Vec::new(&env);
            assets.push_back(StellarAsset {
                asset_code: String::from_str(&env, "XLM"),
                issuer: Some(Address::generate(&env)),
            });
            let mut milestones: Vec<MilestoneData> = Vec::new(&env);
            milestones.push_back(MilestoneData {
                index: 0,
                target_amount: goal,
                released_amount: 0,
                description_hash: BytesN::from_array(&env, &[1u8; 32]),
                status: MilestoneStatus::Locked,
                released_at: None,
                released_at_ledger: None,
                release_tx: None,
                released_to: None,
            });
            let end_time = env.ledger().timestamp() + 86_400;
            CampaignContract::initialize(
                env.clone(),
                creator,
                goal,
                end_time,
                assets,
                milestones,
                0,
            ).expect("initialize valid");

            let campaign = get_campaign(&env).expect("campaign exists");
            let last_idx = campaign.milestone_count - 1;
            let last = get_milestone(&env, last_idx).expect("milestone exists");
            assert_eq!(last.target_amount, campaign.goal_amount);
            assert_eq!(campaign.goal_amount, goal);
        });
    }

    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let mut assets: Vec<StellarAsset> = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "XLM"),
            issuer: Some(Address::generate(&env)),
        });
        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        milestones.push_back(MilestoneData {
            index: 0,
            target_amount: 999, // != goal
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[1u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        });
        let end_time = env.ledger().timestamp() + 86_400;
        CampaignContract::initialize(
            env.clone(),
            creator,
            1000,
            end_time,
            assets,
            milestones,
            0,
        ).unwrap();
    });
}

// ─── Invariant 2: overfunding handled correctly ───────────────────────────────

#[test]
fn invariant_overfunding_handled_correctly() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let (creator, assets, milestones) = setup_basic_campaign(&env);
        let goal_amount: i128 = 1000;
        let end_time = env.ledger().timestamp() + 86_400;

        CampaignContract::initialize(
            env.clone(),
            creator,
            goal_amount,
            end_time,
            assets,
            milestones,
            0,
        ).unwrap();

        let donations = [100i128, 200, 300, 400, 500, 600];
        let mut running_sum: i128 = 0;
        for &amt in &donations {
            let donor = Address::generate(&env);
            running_sum += amt;
            CampaignContract::donate(env.clone(), donor, amt, AssetInfo::Native);

            let campaign = get_campaign(&env).expect("campaign");
            assert_eq!(campaign.raised_amount, running_sum);

            if running_sum >= goal_amount {
                assert_eq!(campaign.status, CampaignStatus::GoalReached);
            } else {
                assert_eq!(campaign.status, CampaignStatus::Active);
            }

            let milestone = get_milestone(&env, 0).expect("milestone");
            if running_sum >= milestone.target_amount {
                assert_eq!(milestone.status, MilestoneStatus::Unlocked);
            } else {
                assert_eq!(milestone.status, MilestoneStatus::Locked);
            }
        }
    });
}

// ─── Invariant 3: total donations match raised ────────────────────────────────

#[test]
fn invariant_total_donations_match_raised() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let (creator, assets, milestones) = setup_basic_campaign(&env);
        let goal_amount: i128 = 1000;
        let end_time = env.ledger().timestamp() + 86_400;

        CampaignContract::initialize(
            env.clone(),
            creator,
            goal_amount,
            end_time,
            assets,
            milestones,
            0,
        ).unwrap();

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);
        let donor3 = Address::generate(&env);

        CampaignContract::donate(env.clone(), donor1.clone(), 300, AssetInfo::Native);
        CampaignContract::donate(env.clone(), donor2.clone(), 400, AssetInfo::Native);
        CampaignContract::donate(env.clone(), donor3.clone(), 300, AssetInfo::Native);

        let d1 = CampaignContract::get_donor_record(env.clone(), donor1).expect("d1");
        let d2 = CampaignContract::get_donor_record(env.clone(), donor2).expect("d2");
        let d3 = CampaignContract::get_donor_record(env.clone(), donor3).expect("d3");

        let sum = d1.total_donated + d2.total_donated + d3.total_donated;
        let campaign = get_campaign(&env).expect("campaign");
        assert_eq!(sum, campaign.raised_amount);

        let total_raised = CampaignContract::get_total_raised(env.clone());
        assert_eq!(total_raised, campaign.raised_amount);
    });
}

// ─── Invariant 4: no released milestones while active ─────────────────────────

#[test]
fn invariant_no_released_milestones_while_active() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let goal_amount: i128 = 3000;
        let end_time = env.ledger().timestamp() + 86_400;

        let mut assets: Vec<StellarAsset> = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "XLM"),
            issuer: Some(Address::generate(&env)),
        });

        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        for i in 0..3 {
            milestones.push_back(MilestoneData {
                index: i,
                target_amount: (i as i128 + 1) * 1000,
                released_amount: 0,
                description_hash: BytesN::from_array(&env, &[(i + 1) as u8; 32]),
                status: MilestoneStatus::Locked,
                released_at: None,
                released_at_ledger: None,
                release_tx: None,
                released_to: None,
            });
        }

        CampaignContract::initialize(
            env.clone(),
            creator,
            goal_amount,
            end_time,
            assets,
            milestones,
            0,
        ).unwrap();

        // Boundary check: stay just below the first unlock threshold.
        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, 999, AssetInfo::Native);

        let campaign = get_campaign(&env).expect("campaign");
        assert_eq!(campaign.status, CampaignStatus::Active);

        let milestone0 = get_milestone(&env, 0).expect("milestone 0");
        assert_eq!(milestone0.status, MilestoneStatus::Locked);
        assert!(milestone0.status != MilestoneStatus::Released);

        // Cross the unlock threshold exactly.
        CampaignContract::donate(env.clone(), Address::generate(&env), 1, AssetInfo::Native);

        let campaign = get_campaign(&env).expect("campaign");
        assert!(campaign.status == CampaignStatus::Active || campaign.status == CampaignStatus::GoalReached);

        let milestone0 = get_milestone(&env, 0).expect("milestone 0");
        assert_eq!(milestone0.status, MilestoneStatus::Unlocked);
        assert!(milestone0.status != MilestoneStatus::Released);

        for i in 0..campaign.milestone_count {
            let ms = get_milestone(&env, i).expect("milestone");
            assert!(ms.status != MilestoneStatus::Released);
        }
    });
}

// ─── Invariant 5: milestone targets strictly ascending ────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #15)")]
fn invariant_milestone_targets_strictly_ascending() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let goal_amount: i128 = 3000;
        let end_time = env.ledger().timestamp() + 86_400;

        let mut assets: Vec<StellarAsset> = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "XLM"),
            issuer: Some(Address::generate(&env)),
        });

        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        for i in 0..3 {
            milestones.push_back(MilestoneData {
                index: i,
                target_amount: (i as i128 + 1) * 1000,
                released_amount: 0,
                description_hash: BytesN::from_array(&env, &[(i + 1) as u8; 32]),
                status: MilestoneStatus::Locked,
                released_at: None,
                released_at_ledger: None,
                release_tx: None,
                released_to: None,
            });
        }

        CampaignContract::initialize(
            env.clone(),
            creator,
            goal_amount,
            end_time,
            assets,
            milestones,
            0,
        ).unwrap();

        let campaign = get_campaign(&env).expect("campaign");
        for i in 0..(campaign.milestone_count - 1) {
            let m0 = get_milestone(&env, i).expect("m0");
            let m1 = get_milestone(&env, i + 1).expect("m1");
            assert!(m0.target_amount < m1.target_amount);
        }
    });

    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let goal_amount: i128 = 3000;
        let end_time = env.ledger().timestamp() + 86_400;

        let mut assets: Vec<StellarAsset> = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "XLM"),
            issuer: Some(Address::generate(&env)),
        });

        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        milestones.push_back(MilestoneData {
            index: 0,
            target_amount: 1000,
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[1u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        });
        milestones.push_back(MilestoneData {
            index: 1,
            target_amount: 1000, // not strictly >
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[2u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        });
        milestones.push_back(MilestoneData {
            index: 2,
            target_amount: 3000,
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[3u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        });

        CampaignContract::initialize(
            env.clone(),
            creator,
            goal_amount,
            end_time,
            assets,
            milestones,
            0,
        ).unwrap();
    });
}

// ─── Invariant 6: random donation sequence preserves state ────────────────────

/// This sequence intentionally overshoots `goal_amount` (3500 vs 3000) to
/// verify state stays internally consistent even after goal reach. It
/// complements `invariant_overfunding_handled_correctly` by checking the same
/// overfunding behavior in a multi-step, multi-milestone scenario.
#[test]
fn invariant_random_donation_sequence_preserves_state() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let goal_amount: i128 = 3000;
        let end_time = env.ledger().timestamp() + 86_400;

        let mut assets: Vec<StellarAsset> = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "XLM"),
            issuer: Some(Address::generate(&env)),
        });

        let mut milestones: Vec<MilestoneData> = Vec::new(&env);
        for i in 0..3 {
            milestones.push_back(MilestoneData {
                index: i,
                target_amount: (i as i128 + 1) * 1000,
                released_amount: 0,
                description_hash: BytesN::from_array(&env, &[(i + 1) as u8; 32]),
                status: MilestoneStatus::Locked,
                released_at: None,
                released_at_ledger: None,
                release_tx: None,
                released_to: None,
            });
        }

        CampaignContract::initialize(
            env.clone(),
            creator,
            goal_amount,
            end_time,
            assets,
            milestones,
            0,
        ).unwrap();

        let amounts = [150i128, 250, 600, 200, 800, 500, 500, 500];
        let mut running_sum = 0i128;
        for &amt in &amounts {
            let donor = Address::generate(&env);
            running_sum += amt;
            CampaignContract::donate(env.clone(), donor, amt, AssetInfo::Native);

            let campaign = get_campaign(&env).expect("campaign");
            assert!(campaign.raised_amount >= 0);
            assert_eq!(campaign.raised_amount, running_sum);
            assert!(
                campaign.status == CampaignStatus::Active ||
                campaign.status == CampaignStatus::GoalReached
            );

            for i in 0..campaign.milestone_count {
                let milestone = get_milestone(&env, i).expect("milestone");
                if campaign.raised_amount >= milestone.target_amount {
                    assert!(milestone.status != MilestoneStatus::Locked);
                }
            }
        }
    });
}

// ─── Invariant 7: single release does not overpay across assets ──────────────

/// This test exercises `release_milestone()` (the single-asset-loop path), NOT
/// `release_milestone_multi_asset()`. It is expected to fail against current
/// contract code, which transfers the full `release_amount` once per accepted
/// asset with a positive balance rather than dividing it proportionally,
/// resulting in total outflow of `target_amount * funded_asset_count` instead
/// of `target_amount`.
#[ignore = "Known bug: release_milestone() overpays by target_amount per funded asset instead of target_amount total. See issue #25. Run with `cargo test -- --ignored` to reproduce."]
#[test]
fn invariant_release_milestone_does_not_overpay_across_assets() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        // Exact pattern copied from release_milestone_tests.rs:
        // - register SACs with register_stellar_asset_contract_v2 (returns a contract wrapper)
        //   *inside* with_contract, then take its address
        // - build campaign via direct set_campaign (like create_test_campaign)
        // - mint via StellarAssetClient inside with_contract using current_contract_address
        // - call the module function directly (not the Client wrapper)
        let creator = Address::generate(&env);
        let token_admin_1 = Address::generate(&env);
        let token_admin_2 = Address::generate(&env);
        let issuer_1 = env
            .register_stellar_asset_contract_v2(token_admin_1.clone())
            .address();
        let issuer_2 = env
            .register_stellar_asset_contract_v2(token_admin_2.clone())
            .address();

        let mut assets: Vec<StellarAsset> = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "AAA"),
            issuer: Some(issuer_1.clone()),
        });
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "BBB"),
            issuer: Some(issuer_2.clone()),
        });

        let campaign = CampaignData {
            creator: creator.clone(),
            goal_amount: 2000,
            raised_amount: 2000,
            end_time: env.ledger().timestamp() + 86_400,
            status: CampaignStatus::GoalReached,
            accepted_assets: assets.clone(),
            milestone_count: 1,
            min_donation_amount: 0,
            created_at_ledger: env.ledger().sequence(),
            created_at_time: env.ledger().timestamp(),
            concluded_at_ledger: None,
        };
        set_campaign(&env, &campaign);

        let milestone = MilestoneData {
            index: 0,
            target_amount: 2000,
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[9u8; 32]),
            status: MilestoneStatus::Unlocked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        };
        set_milestone(&env, 0, &milestone);

        // Seed the per-asset and total raised bookkeeping used by multi-asset release
        storage_set_total_raised(&env, 2000);
        storage_set_asset_raised(&env, &issuer_1, 2000);
        // issuer_2 has 0 raised (all donation came via AAA)

        // Mint funding to the contract (exact mint pattern from mint_tokens_to_contract)
        let asset_client_1 = StellarAssetClient::new(&env, &issuer_1);
        asset_client_1.mint(&env.current_contract_address(), &4000i128);
        let asset_client_2 = StellarAssetClient::new(&env, &issuer_2);
        asset_client_2.mint(&env.current_contract_address(), &4000i128);

        let recipient = Address::generate(&env);
        let before_1 = asset_client_1.balance(&env.current_contract_address());
        let before_2 = asset_client_2.balance(&env.current_contract_address());

        // Direct module call — this is the single-asset-loop release path.
        crate::release_milestone::release_milestone(&env, 0, recipient);

        let after_1 = asset_client_1.balance(&env.current_contract_address());
        let after_2 = asset_client_2.balance(&env.current_contract_address());
        let total_transferred_out = (before_1 - after_1) + (before_2 - after_2);
        assert_eq!(total_transferred_out, milestone.target_amount);
    });
}
