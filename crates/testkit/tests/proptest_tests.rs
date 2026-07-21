//! Property-based invariant tests for the campaign contract.
//!
//! Uses `proptest` to verify that critical accounting invariants hold under
//! randomized sequences of `donate` / `claim_refund` operations.  One test
//! per public method.

use proptest::prelude::*;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

use orbitchain_campaign::types::*;
use orbitchain_campaign::CampaignContract;

use orbitchain_testkit::{
    compute_milestone_targets, make_milestones, with_contract, xlm_assets, BASE,
};

// ─── Strategies ────────────────────────────────────────────────────────────────

fn arb_goal() -> impl Strategy<Value = i128> {
    100i128..1_000_000i128
}

fn arb_deadline_offset() -> impl Strategy<Value = u64> {
    86_400u64..(86_400 * 365)
}

fn arb_milestone_count() -> impl Strategy<Value = usize> {
    1usize..=5
}

fn arb_donation() -> impl Strategy<Value = i128> {
    1i128..100_000i128
}

fn arb_donations() -> impl Strategy<Value = std::vec::Vec<i128>> {
    proptest::collection::vec(arb_donation(), 1..=10)
}

// ─── Helper: initialize a campaign ─────────────────────────────────────────────

fn setup_campaign(env: &Env, goal: i128, milestone_count: usize) {
    let (assets, _issuer) = xlm_assets(env);
    let creator = Address::generate(env);
    let end_time = BASE + 86_400;
    let targets = compute_milestone_targets(goal, milestone_count);
    let milestones = make_milestones(env, &targets);
    CampaignContract::initialize(env.clone(), creator, goal, end_time, assets, milestones, 0)
        .unwrap();
}

// ─── 1. initialize ─────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_initialize_succeeds_with_valid_params(
        goal in arb_goal(),
        offset in arb_deadline_offset(),
        milestone_count in arb_milestone_count(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let (assets, _issuer) = xlm_assets(&env);
            let creator = Address::generate(&env);
            let end_time = BASE + offset;
            let targets = compute_milestone_targets(goal, milestone_count);
            let milestones = make_milestones(&env, &targets);

            let result = CampaignContract::initialize(
                env.clone(),
                creator,
                goal,
                end_time,
                assets,
                milestones,
                0,
            );
            assert!(result.is_ok(), "initialize should succeed");

            let campaign = orbitchain_campaign::storage::get_campaign(&env).unwrap();
            assert_eq!(campaign.goal_amount, goal);
            assert_eq!(campaign.status, CampaignStatus::Active);
            assert_eq!(campaign.raised_amount, 0);
            assert_eq!(campaign.milestone_count, milestone_count as u32);

            let last = orbitchain_campaign::storage::get_milestone(
                &env,
                campaign.milestone_count - 1,
            )
            .unwrap();
            assert_eq!(last.target_amount, goal);
        });
    }
}

// ─── 2. donate — increases total_raised ────────────────────────────────────────

proptest! {
    #[test]
    fn prop_donate_increases_total_raised(
        amounts in arb_donations(),
        goal_multiplier in 2i128..10i128,
    ) {
        let goal_base: i128 = amounts.iter().sum::<i128>().max(1);
        let goal = (goal_base * goal_multiplier).max(100);
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            let mut expected: i128 = 0;
            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
                expected += amount;
                let actual = CampaignContract::get_total_raised(env.clone());
                assert_eq!(actual, expected);
            }
        });
    }
}

// ─── 3. donate — milestone unlock ──────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_donate_unlocks_milestones_at_threshold(
        amounts in arb_donations(),
        milestone_count in arb_milestone_count(),
    ) {
        let sum: i128 = amounts.iter().sum();
        let goal = (sum * 3).max(100);
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, milestone_count);

            let mut cumulative: i128 = 0;
            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
                cumulative += amount;

                for i in 0..milestone_count as u32 {
                    let ms = CampaignContract::get_milestone_view(env.clone(), i);
                    if cumulative >= ms.target_amount {
                        assert!(
                            ms.status == MilestoneStatus::Unlocked
                                || ms.status == MilestoneStatus::Released,
                            "milestone {} (target {}) should be unlocked at cumulative {}",
                            i,
                            ms.target_amount,
                            cumulative,
                        );
                    }
                }
            }
        });
    }
}

// ─── 4. donate — goal reached transition ───────────────────────────────────────

proptest! {
    #[test]
    fn prop_donate_transitions_to_goal_reached(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let total: i128 = amounts.iter().sum();
        let goal = total;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let campaign = orbitchain_campaign::storage::get_campaign(&env).unwrap();
            assert_eq!(campaign.status, CampaignStatus::GoalReached);
        });
    }
}

// ─── 5. get_total_raised ───────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_total_raised_matches_donations(
        amounts in arb_donations(),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let expected: i128 = amounts.iter().sum();
            let actual = CampaignContract::get_total_raised(env.clone());
            assert_eq!(actual, expected);
        });
    }
}

// ─── 6. get_donation_count ─────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_donation_count_matches_donations(
        amounts in arb_donations(),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let count = CampaignContract::get_donation_count(env.clone());
            assert_eq!(count, amounts.len() as u64);
        });
    }
}

// ─── 7. get_donor_count ────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_donor_count_unique_donors(
        amounts in proptest::collection::vec(arb_donation(), 1..=8),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let donor_count = CampaignContract::get_donor_count(env.clone());
            assert_eq!(donor_count, amounts.len() as u32);
        });
    }
}

// ─── 8. get_release_count ──────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_release_count_starts_at_zero(
        amounts in proptest::collection::vec(arb_donation(), 0..=3),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let count = CampaignContract::get_release_count(env.clone());
            assert_eq!(count, 0);
        });
    }
}

// ─── 9. get_total_tx_count ─────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_total_tx_count_equals_donations(
        amounts in arb_donations(),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let total_tx = CampaignContract::get_total_tx_count(env.clone());
            let donations = CampaignContract::get_donation_count(env.clone());
            let releases = CampaignContract::get_release_count(env.clone());
            assert_eq!(total_tx, donations + releases);
        });
    }
}

// ─── 10. get_campaign_report ───────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_campaign_report_consistent(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let total: i128 = amounts.iter().sum();
        let goal = total.max(100) * 2;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let (assets, _issuer) = xlm_assets(&env);
            let creator = Address::generate(&env);
            let end_time = BASE + 86_400;
            let targets = compute_milestone_targets(goal, 1);
            let milestones = make_milestones(&env, &targets);

            CampaignContract::initialize(
                env.clone(),
                creator.clone(),
                goal,
                end_time,
                assets,
                milestones,
                0,
            )
            .unwrap();

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let report = CampaignContract::get_campaign_report(env.clone()).unwrap();
            assert_eq!(report.creator, creator);
            assert_eq!(report.goal_amount, goal);
            assert_eq!(report.raised_amount, total);
            assert_eq!(report.remaining_amount, (goal - total).max(0));
            assert_eq!(report.donation_count, amounts.len() as u64);
            assert_eq!(report.milestone_count, 1);

            if total >= goal {
                assert_eq!(report.progress_bps, 10_000);
            } else {
                let expected_bps = (total * 10_000 / goal) as u32;
                assert_eq!(report.progress_bps, expected_bps);
            }
        });
    }
}

// ─── 11. get_platform_summary ──────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_platform_summary_after_donations(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let summary = CampaignContract::get_platform_summary(env.clone());
            assert_eq!(summary.total_campaigns, 1);
            assert_eq!(summary.active_campaigns, 1);
            assert_eq!(summary.total_donations, amounts.len() as u64);
            assert_eq!(summary.total_releases, 0);
            assert_eq!(summary.total_transactions, amounts.len() as u64);
        });
    }
}

// ─── 12. get_dashboard_metrics ─────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_dashboard_metrics_matches_summary(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            let summary = CampaignContract::get_platform_summary(env.clone());
            let metrics = CampaignContract::get_dashboard_metrics(env.clone());
            assert_eq!(metrics.total_campaigns, summary.total_campaigns);
            assert_eq!(metrics.active_campaigns, summary.active_campaigns);
            assert_eq!(metrics.total_donations, summary.total_donations);
            assert_eq!(metrics.total_releases, summary.total_releases);
            assert_eq!(metrics.total_transactions, summary.total_transactions);
        });
    }
}

// ─── 13. get_donor_record ──────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_donor_record_after_donate(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            let mut donors: std::vec::Vec<Address> = std::vec::Vec::new();
            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor.clone(), *amount, AssetInfo::Native);
                donors.push(donor);
            }

            for (i, donor) in donors.iter().enumerate() {
                let record = CampaignContract::get_donor_record(env.clone(), donor.clone());
                assert!(record.is_some(), "donor {} should have a record", i);
                let record = record.unwrap();
                assert_eq!(record.total_donated, amounts[i]);
                assert_eq!(record.donation_count, 1);
            }

            let non_donor = Address::generate(&env);
            assert!(
                CampaignContract::get_donor_record(env.clone(), non_donor).is_none()
            );
        });
    }
}

// ─── 14. hello ─────────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_hello_always_returns_campaign(_seed in 0u64..100) {
        let env = Env::default();
        let result = CampaignContract::hello(env.clone());
        assert_eq!(result, soroban_sdk::Symbol::new(&env, "campaign"));
    }
}

// ─── 15. version ───────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_version_always_returns_one(_seed in 0u64..100) {
        assert_eq!(CampaignContract::version(), 1);
    }
}

// ─── 16. version_str ───────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_version_str_always_returns_0_1_0(_seed in 0u64..100) {
        let env = Env::default();
        let result = CampaignContract::version_str(env.clone());
        let expected = soroban_sdk::String::from_str(&env, "0.1.0");
        assert_eq!(result, expected);
    }
}

// ─── 17. is_refund_eligible ────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_is_refund_eligible_after_end_no_release(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let total: i128 = amounts.iter().sum();
        let goal = total.max(100) * 2;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let (assets, _issuer) = xlm_assets(&env);
            let creator = Address::generate(&env);
            let end_time = BASE + 86_400;
            let targets = compute_milestone_targets(goal, 1);
            let milestones = make_milestones(&env, &targets);

            CampaignContract::initialize(
                env.clone(),
                creator,
                goal,
                end_time,
                assets,
                milestones,
                0,
            )
            .unwrap();

            let mut donors: std::vec::Vec<Address> = std::vec::Vec::new();
            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor.clone(), *amount, AssetInfo::Native);
                donors.push(donor);
            }

            for donor in &donors {
                assert!(
                    !CampaignContract::is_refund_eligible(env.clone(), donor.clone()),
                    "should not be eligible while active"
                );
            }

            CampaignContract::end_campaign(env.clone());

            for donor in &donors {
                assert!(
                    CampaignContract::is_refund_eligible(env.clone(), donor.clone()),
                    "should be eligible after end"
                );
            }
        });
    }
}

// ─── 18. get_campaign_status ───────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_campaign_status_reflects_state(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
        should_end in prop::bool::ANY,
    ) {
        let total: i128 = amounts.iter().sum();
        let goal = total.max(100) * 2;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            if should_end {
                CampaignContract::end_campaign(env.clone());
                let status = CampaignContract::get_campaign_status(env.clone());
                assert_eq!(status.status, CampaignStatus::Ended);
            } else {
                let status = CampaignContract::get_campaign_status(env.clone());
                assert!(
                    status.status == CampaignStatus::Active
                        || status.status == CampaignStatus::GoalReached,
                    "unexpected status: {:?}",
                    status.status,
                );
            }
        });
    }
}

// ─── 19. get_milestone_view ────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_milestone_view_matches_targets(
        goal in arb_goal(),
        milestone_count in arb_milestone_count(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let targets = compute_milestone_targets(goal, milestone_count);
            setup_campaign(&env, goal, milestone_count);

            for i in 0..milestone_count as u32 {
                let ms = CampaignContract::get_milestone_view(env.clone(), i);
                assert_eq!(ms.target_amount, targets[i as usize]);
                assert_eq!(ms.status, MilestoneStatus::Locked);
                assert_eq!(ms.released_amount, 0);
            }
        });
    }
}

// ─── 20. get_all_milestones ────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_get_all_milestones_count_matches(
        goal in arb_goal(),
        milestone_count in arb_milestone_count(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, milestone_count);

            let all = CampaignContract::get_all_milestones(env.clone());
            assert_eq!(all.len() as usize, milestone_count);
        });
    }
}

// ─── 21. end_campaign ──────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_end_campaign_sets_ended(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            CampaignContract::end_campaign(env.clone());

            let campaign = orbitchain_campaign::storage::get_campaign(&env).unwrap();
            assert_eq!(campaign.status, CampaignStatus::Ended);
            assert!(campaign.concluded_at_ledger.is_some());
        });
    }
}

// ─── 22. cancel_campaign ───────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_cancel_campaign_sets_cancelled(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
            }

            CampaignContract::cancel_campaign(env.clone());

            let campaign = orbitchain_campaign::storage::get_campaign(&env).unwrap();
            assert_eq!(campaign.status, CampaignStatus::Cancelled);
            assert!(campaign.concluded_at_ledger.is_some());
        });
    }
}

// ─── 23. extend_deadline ───────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_extend_deadline_updates_end_time(
        initial_offset in 86_400u64..(86_400 * 30),
        extension in 86_400u64..(86_400 * 365),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let (assets, _issuer) = xlm_assets(&env);
            let creator = Address::generate(&env);
            let end_time = BASE + initial_offset;
            let targets = vec![10_000i128];
            let milestones = make_milestones(&env, &targets);

            CampaignContract::initialize(
                env.clone(),
                creator,
                10_000,
                end_time,
                assets,
                milestones,
                0,
            )
            .unwrap();

            let new_end_time = env.ledger().timestamp() + extension;
            CampaignContract::extend_deadline(env.clone(), new_end_time);

            let campaign = orbitchain_campaign::storage::get_campaign(&env).unwrap();
            assert_eq!(campaign.end_time, new_end_time);
        });
    }
}

// ─── 24. claim_refund eligibility after cancel ────────────────────────────────

proptest! {
    #[test]
    fn prop_claim_refund_eligibility_after_cancel(
        amounts in proptest::collection::vec(arb_donation(), 1..=5),
    ) {
        let total: i128 = amounts.iter().sum();
        let goal = total.max(100) * 2;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let (assets, _issuer) = xlm_assets(&env);
            let creator = Address::generate(&env);
            let end_time = BASE + 86_400;
            let targets = compute_milestone_targets(goal, 1);
            let milestones = make_milestones(&env, &targets);

            CampaignContract::initialize(
                env.clone(),
                creator,
                goal,
                end_time,
                assets,
                milestones,
                0,
            )
            .unwrap();

            let mut donors: std::vec::Vec<Address> = std::vec::Vec::new();
            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor.clone(), *amount, AssetInfo::Native);
                donors.push(donor);
            }

            CampaignContract::cancel_campaign(env.clone());

            for donor in &donors {
                assert!(
                    CampaignContract::is_refund_eligible(env.clone(), donor.clone()),
                    "donor should be eligible after cancel"
                );
            }

            let non_donor = Address::generate(&env);
            assert!(
                !CampaignContract::is_refund_eligible(env.clone(), non_donor),
                "non-donor should not be eligible"
            );
        });
    }
}

// ─── 25. get_donor_record — non-donor returns None ────────────────────────────

proptest! {
    #[test]
    fn prop_get_donor_record_non_donor(_seed in 0u64..100) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let non_donor = Address::generate(&env);
            assert!(
                CampaignContract::get_donor_record(env.clone(), non_donor).is_none()
            );
        });
    }
}

// ─── 26. bump_storage ──────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_bump_storage_no_panic(
        goal in arb_goal(),
        milestone_count in arb_milestone_count(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, milestone_count);
            CampaignContract::bump_storage(env.clone());
            CampaignContract::bump_storage(env.clone());
        });
    }
}

// ─── 27. is_asset_blocked_view ─────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_is_asset_blocked_view_default_false(_seed in 0u64..100) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            let asset = Address::generate(&env);
            assert!(
                !CampaignContract::is_asset_blocked_view(env.clone(), asset),
                "asset should not be blocked by default"
            );
        });
    }
}

// ─── 28. freeze / unfreeze ─────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_freeze_unfreeze_roundtrip(_seed in 0u64..100) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, 10_000, 1);

            CampaignContract::freeze(env.clone());
            assert!(
                orbitchain_campaign::storage::is_frozen(&env),
                "should be frozen"
            );

            CampaignContract::unfreeze(env.clone());
            assert!(
                !orbitchain_campaign::storage::is_frozen(&env),
                "should be unfrozen"
            );
        });
    }
}

// ─── 29. block_asset / unblock_asset ───────────────────────────────────────────

proptest! {
    #[test]
    fn prop_block_unblock_asset_roundtrip(_seed in 0u64..100) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, 10_000, 1);

            let asset = Address::generate(&env);

            assert!(
                !CampaignContract::is_asset_blocked_view(env.clone(), asset.clone()),
            );

            CampaignContract::block_asset(env.clone(), asset.clone());
            assert!(
                CampaignContract::is_asset_blocked_view(env.clone(), asset.clone()),
            );

            CampaignContract::unblock_asset(env.clone(), asset.clone());
            assert!(
                !CampaignContract::is_asset_blocked_view(env.clone(), asset.clone()),
            );
        });
    }
}

// ─── 30. invariant: raised_amount never exceeds goal ───────────────────────────

proptest! {
    #[test]
    fn prop_invariant_raised_never_exceeds_goal(
        amounts in proptest::collection::vec(arb_donation(), 1..=10),
    ) {
        let total: i128 = amounts.iter().sum();
        let goal = total.max(100);
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);

                let campaign = orbitchain_campaign::storage::get_campaign(&env).unwrap();
                assert!(
                    campaign.raised_amount <= campaign.goal_amount,
                    "INVARIANT: raised_amount ({}) > goal_amount ({})",
                    campaign.raised_amount,
                    campaign.goal_amount,
                );
            }
        });
    }
}

// ─── 31. invariant: sum of donations == raised_amount ──────────────────────────

proptest! {
    #[test]
    fn prop_invariant_sum_donations_equals_raised(
        amounts in proptest::collection::vec(arb_donation(), 1..=10),
    ) {
        let goal: i128 = amounts.iter().sum::<i128>().max(100) * 10;
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, 1);

            let mut cumulative: i128 = 0;
            for amount in &amounts {
                let donor = Address::generate(&env);
                CampaignContract::donate(env.clone(), donor, *amount, AssetInfo::Native);
                cumulative += amount;

                let raised = CampaignContract::get_total_raised(env.clone());
                assert_eq!(raised, cumulative);

                let campaign = orbitchain_campaign::storage::get_campaign(&env).unwrap();
                assert_eq!(campaign.raised_amount, cumulative);
            }
        });
    }
}

// ─── 32. invariant: milestone targets strictly ascending ────────────────────────

proptest! {
    #[test]
    fn prop_invariant_milestone_targets_ascending(
        goal in arb_goal(),
        milestone_count in arb_milestone_count(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        with_contract(&env, || {
            setup_campaign(&env, goal, milestone_count);

            let mut prev: i128 = 0;
            for i in 0..milestone_count as u32 {
                let ms = CampaignContract::get_milestone_view(env.clone(), i);
                assert!(
                    ms.target_amount > prev,
                    "INVARIANT: milestone {} target ({}) not > prev ({})",
                    i,
                    ms.target_amount,
                    prev,
                );
                prev = ms.target_amount;
            }

            let last = CampaignContract::get_milestone_view(
                env.clone(),
                milestone_count as u32 - 1,
            );
            assert_eq!(last.target_amount, goal);
        });
    }
}
