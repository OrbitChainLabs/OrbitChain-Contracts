use soroban_sdk::Env;

use crate::storage::{get_campaign_or_panic, get_milestone};
use crate::types::{Error, MilestoneData};
use crate::views::{get_milestone_by_index, MilestoneView};

// ─── get_milestone_view ───────────────────────────────────────────────────────

/// Issue #199 — Returns the raw `MilestoneData` for the milestone at `index`.
///
/// Prefer [`get_milestone_view_enriched`] when you need computed fields
/// (`pending_release`, `is_fully_released`, `is_next_pending`).
/// Use this only when you need the bare stored record without the overhead
/// of deriving enriched fields.
///
/// No authentication required (read-only view).
///
/// # Panics
///   `Error::NotInitialized`    — contract not yet initialised.
///   `Error::MilestoneNotFound` — `index` ≥ `milestone_count`, or the entry
///                                is absent from storage (corrupted state).
#[must_use]
pub fn get_milestone_view(env: &Env, index: u32) -> MilestoneData {
    let campaign = get_campaign_or_panic(env);

    if index >= campaign.milestone_count {
        soroban_sdk::panic_with_error!(env, Error::MilestoneNotFound);
    }

    get_milestone(env, index)
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, Error::MilestoneNotFound))
}

// ─── get_milestone_view_enriched ─────────────────────────────────────────────

/// Returns the enriched [`MilestoneView`] for `index`.
///
/// A thin wrapper around [`crate::views::get_milestone_by_index`] kept here
/// so Issue #199 callers have a single import point for both raw and enriched
/// variants.  Prefer this over [`get_milestone_view`] unless you specifically
/// need the bare `MilestoneData` record.
///
/// No authentication required (read-only view).
///
/// # Panics
///   `Error::NotInitialized`    — contract not yet initialised.
///   `Error::MilestoneNotFound` — `index` ≥ `milestone_count` or missing
///                                from storage.
#[must_use]
pub fn get_milestone_view_enriched(env: &Env, index: u32) -> MilestoneView {
    get_milestone_by_index(env, index)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::test::with_contract;
    use crate::types::{CampaignData, CampaignStatus, DataKey, MilestoneStatus};

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_env() -> Env {
        Env::default()
    }

    /// Seeds a minimal `CampaignData` so `get_campaign_or_panic` does not fire.
    fn seed_campaign(env: &Env, milestone_count: u32) {
        let campaign = CampaignData {
            creator: Address::generate(env),
            goal_amount: 10_000,
            raised_amount: 0,
            end_time: 9_999_999,
            status: CampaignStatus::Active,
            milestone_count,
            accepted_assets: soroban_sdk::Vec::new(env),
            min_donation_amount: 0,
            created_at_ledger: env.ledger().sequence(),
            created_at_time: env.ledger().timestamp(),
            concluded_at_ledger: None,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CampaignData, &campaign);
    }

    /// Writes a `MilestoneData` directly to persistent storage and returns it
    /// so callers can assert field-by-field.
    fn seed_milestone(env: &Env, index: u32, status: MilestoneStatus) -> MilestoneData {
        let m = MilestoneData {
            index,
            target_amount: (index as i128 + 1) * 1_000,
            released_amount: if status == MilestoneStatus::Released {
                (index as i128 + 1) * 1_000
            } else {
                0
            },
            description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
            status,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneData(index), &m);
        m
    }

    // ── get_milestone_view ───────────────────────────────────────────────────

    #[test]
    fn returns_raw_data_for_index_zero() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 1);
            let stored = seed_milestone(&env, 0, MilestoneStatus::Locked);
            assert_eq!(get_milestone_view(&env, 0), stored);
        });
    }

    #[test]
    fn returns_correct_milestone_for_non_zero_index() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 3);
            seed_milestone(&env, 0, MilestoneStatus::Released);
            seed_milestone(&env, 1, MilestoneStatus::Unlocked);
            let stored = seed_milestone(&env, 2, MilestoneStatus::Locked);
            let result = get_milestone_view(&env, 2);
            assert_eq!(result.index, 2);
            assert_eq!(result.target_amount, stored.target_amount);
            assert_eq!(result.status, MilestoneStatus::Locked);
        });
    }

    #[test]
    #[should_panic]
    fn panics_on_index_equal_to_milestone_count() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 1);
            seed_milestone(&env, 0, MilestoneStatus::Locked);
            let _ = get_milestone_view(&env, 1);
        });
    }

    #[test]
    #[should_panic]
    fn panics_on_index_far_exceeding_milestone_count() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 1);
            seed_milestone(&env, 0, MilestoneStatus::Locked);
            let _ = get_milestone_view(&env, 99);
        });
    }

    #[test]
    #[should_panic]
    fn panics_when_contract_not_initialised() {
        let env = make_env();
        with_contract(&env, || {
            let _ = get_milestone_view(&env, 0);
        });
    }

    // ── get_milestone_view_enriched ──────────────────────────────────────────

    #[test]
    fn enriched_carries_pending_release_and_is_next_pending() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 2);
            seed_milestone(&env, 0, MilestoneStatus::Released);
            let stored = seed_milestone(&env, 1, MilestoneStatus::Unlocked);
            let view = get_milestone_view_enriched(&env, 1);
            assert_eq!(view.data, stored);
            assert_eq!(view.pending_release, stored.target_amount);
            assert!(!view.is_fully_released);
            assert!(view.is_next_pending);
        });
    }

    #[test]
    fn enriched_is_fully_released_when_milestone_is_released() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 1);
            seed_milestone(&env, 0, MilestoneStatus::Released);
            let view = get_milestone_view_enriched(&env, 0);
            assert!(view.is_fully_released);
            assert_eq!(view.pending_release, 0);
            assert!(!view.is_next_pending);
        });
    }

    #[test]
    fn enriched_locked_milestone_is_not_marked_next_pending() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 2);
            seed_milestone(&env, 0, MilestoneStatus::Unlocked);
            seed_milestone(&env, 1, MilestoneStatus::Locked);
            let view = get_milestone_view_enriched(&env, 1);
            assert!(!view.is_next_pending);
        });
    }
}
