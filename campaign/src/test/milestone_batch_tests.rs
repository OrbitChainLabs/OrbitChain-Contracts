//! Tests for the single-Vec milestone layout and the batched unlock path
//! (issue #118), including the acceptance-criterion benchmark: the batched
//! unlock burst must cost >40% less than the per-entry pattern it replaced.

#![cfg(test)]

use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use super::with_contract;
use crate::storage::{
    get_milestone, get_milestones_vec, set_campaign, set_milestone, unlock_milestones_batch,
};
use crate::types::{
    CampaignData, CampaignStatus, DataKey, MilestoneData, MilestoneStatus, StellarAsset,
};

const BURST: u32 = 5; // initialize's maximum milestone count

fn make_env() -> Env {
    let env = Env::default();
    env.ledger().with_mut(|l| {
        l.sequence_number = 1_000;
        l.max_entry_ttl = 10_000_000;
        l.min_persistent_entry_ttl = 100;
    });
    env
}

fn milestone(env: &Env, index: u32, target: i128) -> MilestoneData {
    MilestoneData {
        index,
        target_amount: target,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[7u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    }
}

fn setup_campaign(env: &Env, milestone_count: u32) {
    let creator = Address::generate(env);
    let campaign = CampaignData {
        creator,
        goal_amount: 1_000 * milestone_count as i128,
        raised_amount: 0,
        end_time: env.ledger().timestamp() + 86_400 * 30,
        status: CampaignStatus::Active,
        accepted_assets: {
            let mut assets: Vec<StellarAsset> = Vec::new(env);
            assets.push_back(StellarAsset {
                asset_code: String::from_str(env, "XLM"),
                issuer: Some(Address::generate(env)),
            });
            assets
        },
        milestone_count,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    };
    set_campaign(env, &campaign);
}

/// Ascending targets 1000, 2000, … as initialize validates.
fn seed_vec_layout(env: &Env, count: u32) {
    for i in 0..count {
        set_milestone(env, i, &milestone(env, i, 1_000 * (i as i128 + 1)));
    }
}

/// The pre-#118 layout: one persistent entry per index, no MilestonesVec.
fn seed_legacy_layout(env: &Env, count: u32) {
    for i in 0..count {
        env.storage().persistent().set(
            &DataKey::MilestoneData(i),
            &milestone(env, i, 1_000 * (i as i128 + 1)),
        );
    }
}

/// Replica of the unlock loop this change replaced: per-index get + set
/// against the legacy layout, with the same TTL-bump behaviour the old
/// `get_milestone`/`set_milestone` had. Kept in-tree so the benchmark below
/// compares the real before/after shapes forever, not a guess.
fn old_unlock_loop(env: &Env, milestone_count: u32, raised_amount: i128) {
    use crate::storage::{PERSISTENT_BUMP_AMOUNT, PERSISTENT_BUMP_THRESHOLD};
    for i in 0..milestone_count {
        let key = DataKey::MilestoneData(i);
        let got: Option<MilestoneData> = env.storage().persistent().get(&key);
        if let Some(mut m) = got {
            if env.storage().persistent().get_ttl(&key) < PERSISTENT_BUMP_THRESHOLD {
                env.storage().persistent().extend_ttl(
                    &key,
                    PERSISTENT_BUMP_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
            }
            if m.status == MilestoneStatus::Locked && raised_amount >= m.target_amount {
                m.status = MilestoneStatus::Unlocked;
                env.storage().persistent().set(&key, &m);
                if env.storage().persistent().get_ttl(&key) < PERSISTENT_BUMP_THRESHOLD {
                    env.storage().persistent().extend_ttl(
                        &key,
                        PERSISTENT_BUMP_THRESHOLD,
                        PERSISTENT_BUMP_AMOUNT,
                    );
                }
            }
        }
    }
}

#[test]
fn burst_unlocks_every_reached_milestone_in_one_write() {
    let env = make_env();
    with_contract(&env, || {
        setup_campaign(&env, BURST);
        seed_vec_layout(&env, BURST);

        // Raised covers every target → the whole burst unlocks.
        let unlocked = unlock_milestones_batch(&env, 5_000);
        assert_eq!(unlocked.len(), BURST);
        for (i, (index, target)) in unlocked.iter().enumerate() {
            assert_eq!(index, i as u32);
            assert_eq!(target, 1_000 * (i as i128 + 1));
        }
        let v = get_milestones_vec(&env);
        for i in 0..BURST {
            assert_eq!(v.get(i).unwrap().status, MilestoneStatus::Unlocked);
        }
    });
}

#[test]
fn partial_unlock_stops_at_first_unreached_target() {
    let env = make_env();
    with_contract(&env, || {
        setup_campaign(&env, BURST);
        seed_vec_layout(&env, BURST);

        // 2500 covers targets 1000 and 2000 only.
        let unlocked = unlock_milestones_batch(&env, 2_500);
        assert_eq!(unlocked.len(), 2);
        let v = get_milestones_vec(&env);
        assert_eq!(v.get(0).unwrap().status, MilestoneStatus::Unlocked);
        assert_eq!(v.get(1).unwrap().status, MilestoneStatus::Unlocked);
        for i in 2..BURST {
            assert_eq!(v.get(i).unwrap().status, MilestoneStatus::Locked);
        }
    });
}

#[test]
fn already_unlocked_milestones_are_not_reported_again() {
    let env = make_env();
    with_contract(&env, || {
        setup_campaign(&env, 3);
        seed_vec_layout(&env, 3);

        assert_eq!(unlock_milestones_batch(&env, 1_500).len(), 1);
        // Second pass at a higher raise: only the newly reached ones report.
        let second = unlock_milestones_batch(&env, 3_500).len();
        assert_eq!(second, 2);
        // Third pass with nothing new: no unlocks, and (by inspection of the
        // helper) no write happens for an empty result.
        assert_eq!(unlock_milestones_batch(&env, 3_500).len(), 0);
    });
}

#[test]
fn legacy_per_index_layout_reads_and_migrates() {
    let env = make_env();
    with_contract(&env, || {
        setup_campaign(&env, 3);
        seed_legacy_layout(&env, 3);

        // Reads fall back to the legacy entries.
        assert_eq!(get_milestone(&env, 1).unwrap().target_amount, 2_000);
        assert!(!env.storage().persistent().has(&DataKey::MilestonesVec));

        // First write-through migrates the assembled set to the Vec layout.
        let mut m0 = get_milestone(&env, 0).unwrap();
        m0.status = MilestoneStatus::Unlocked;
        set_milestone(&env, 0, &m0);
        assert!(env.storage().persistent().has(&DataKey::MilestonesVec));
        let v = get_milestones_vec(&env);
        assert_eq!(v.len(), 3);
        assert_eq!(v.get(0).unwrap().status, MilestoneStatus::Unlocked);
        assert_eq!(v.get(2).unwrap().target_amount, 3_000);

        // The batch helper also works over the migrated set.
        assert_eq!(unlock_milestones_batch(&env, 10_000).len(), 2);
    });
}

/// Acceptance criterion for #118: >40% improvement in the unlock-burst case.
///
/// Measures host CPU-instruction cost of the replaced per-entry loop vs the
/// batched helper over an identical 5-milestone full-unlock burst. The old
/// shape performs 5 reads + 5 writes (+ TTL probes) against 5 entries; the
/// batched shape performs 1 read + 1 write against a single entry.
#[test]
fn benchmark_burst_unlock_improves_by_more_than_40_percent() {
    // Two identical worlds, one per layout, so state is equivalent.
    let old_env = make_env();
    let old_cost = {
        let mut cost = 0u64;
        with_contract(&old_env, || {
            setup_campaign(&old_env, BURST);
            seed_legacy_layout(&old_env, BURST);
            old_env.budget().reset_default();
            old_unlock_loop(&old_env, BURST, 5_000);
            cost = old_env.budget().cpu_instruction_cost();
        });
        cost
    };

    let new_env = make_env();
    let new_cost = {
        let mut cost = 0u64;
        with_contract(&new_env, || {
            setup_campaign(&new_env, BURST);
            seed_vec_layout(&new_env, BURST);
            new_env.budget().reset_default();
            let unlocked = unlock_milestones_batch(&new_env, 5_000);
            cost = new_env.budget().cpu_instruction_cost();
            assert_eq!(unlocked.len(), BURST);
        });
        cost
    };

    // Fails the build if the batching regresses below the 40% bar.
    assert!(
        new_cost * 100 <= old_cost * 60,
        "unlock burst must be >40% cheaper: old={old_cost} new={new_cost} ({}% of old)",
        new_cost * 100 / old_cost
    );
}
