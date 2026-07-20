//! Tests for `CampaignContract::bump_storage` (issue #120).
//!
//! Verifies that the public TTL-maintenance entrypoint actually extends the
//! TTL of every core persistent key, that it needs no authorization, that it
//! tolerates gaps (declared-but-unwritten milestone slots), and that it
//! panics on an uninitialized campaign.

#![cfg(test)]

use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use super::with_contract;
use crate::storage::{
    set_campaign, set_milestone, storage_set_total_raised, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_BUMP_THRESHOLD,
};
use crate::types::{
    CampaignData, CampaignStatus, DataKey, MilestoneData, MilestoneStatus, StellarAsset,
};
use crate::CampaignContract;

/// Ledger distance to travel after setup so every entry's remaining TTL sinks
/// below `PERSISTENT_BUMP_THRESHOLD` (writes bump to `PERSISTENT_BUMP_AMOUNT`,
/// so we must burn more than AMOUNT - THRESHOLD ledgers to need a re-bump).
const DRIFT: u32 = PERSISTENT_BUMP_AMOUNT - PERSISTENT_BUMP_THRESHOLD + 1;

fn make_env() -> Env {
    let env = Env::default();
    env.ledger().with_mut(|l| {
        l.sequence_number = 1_000;
        // Ample headroom so entries survive the drift and re-bumps.
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
        goal_amount: 1_000,
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

fn remaining_ttl(env: &Env, key: &DataKey) -> u32 {
    env.storage().persistent().get_ttl(key)
}

#[test]
fn bump_storage_extends_core_and_milestone_ttls() {
    let env = make_env();
    // NB: no mock_all_auths() anywhere in this test — the entrypoint is
    // deliberately unauthenticated (see its doc comment).
    with_contract(&env, || {
        setup_campaign(&env, 2);
        set_milestone(&env, 0, &milestone(&env, 0, 500));
        set_milestone(&env, 1, &milestone(&env, 1, 1_000));
        storage_set_total_raised(&env, 42);

        // Writes bump on access, so entries start at full TTL. Advance the
        // ledger until every remaining TTL is below the bump threshold.
        // (Milestones live under the single MilestonesVec key — issue #118.)
        env.ledger().with_mut(|l| l.sequence_number += DRIFT);
        for key in [
            DataKey::CampaignData,
            DataKey::MilestonesVec,
            DataKey::TotalRaised,
        ] {
            assert!(
                remaining_ttl(&env, &key) < PERSISTENT_BUMP_THRESHOLD,
                "precondition: {key:?} must need a bump"
            );
        }

        CampaignContract::bump_storage(env.clone());

        for key in [
            DataKey::CampaignData,
            DataKey::MilestonesVec,
            DataKey::TotalRaised,
        ] {
            assert_eq!(
                remaining_ttl(&env, &key),
                PERSISTENT_BUMP_AMOUNT,
                "{key:?} must be re-extended to the full bump amount"
            );
        }
    });
}

#[test]
fn bump_storage_tolerates_unwritten_milestone_slots() {
    let env = make_env();
    with_contract(&env, || {
        // Declares 3 milestones but only writes the first — the bump must
        // skip missing keys instead of panicking (`extend_ttl` panics on
        // absent entries; `bump_all_persistent` guards with `has()`).
        setup_campaign(&env, 3);
        set_milestone(&env, 0, &milestone(&env, 0, 1_000));

        env.ledger().with_mut(|l| l.sequence_number += DRIFT);
        CampaignContract::bump_storage(env.clone());

        assert_eq!(
            remaining_ttl(&env, &DataKey::MilestonesVec),
            PERSISTENT_BUMP_AMOUNT
        );
    });
}

#[test]
fn bump_storage_still_bumps_legacy_per_index_entries() {
    let env = make_env();
    with_contract(&env, || {
        // A contract initialized before the #118 layout change has one entry
        // per milestone index and no MilestonesVec. bump_storage must keep
        // those alive too (pre-migration contracts).
        setup_campaign(&env, 1);
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneData(0), &milestone(&env, 0, 1_000));

        env.ledger().with_mut(|l| l.sequence_number += DRIFT);
        CampaignContract::bump_storage(env.clone());

        assert_eq!(
            remaining_ttl(&env, &DataKey::MilestoneData(0)),
            PERSISTENT_BUMP_AMOUNT
        );
    });
}

#[test]
fn bump_storage_is_idempotent_at_full_ttl() {
    let env = make_env();
    with_contract(&env, || {
        setup_campaign(&env, 1);
        set_milestone(&env, 0, &milestone(&env, 0, 1_000));

        // Immediately after setup every entry is already at full TTL, so a
        // bump is a harmless no-op (below-threshold check) — calling twice
        // must not change the outcome or panic.
        CampaignContract::bump_storage(env.clone());
        CampaignContract::bump_storage(env.clone());

        assert_eq!(
            remaining_ttl(&env, &DataKey::CampaignData),
            PERSISTENT_BUMP_AMOUNT
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn bump_storage_panics_when_uninitialized() {
    let env = make_env();
    with_contract(&env, || {
        CampaignContract::bump_storage(env.clone());
    });
}
