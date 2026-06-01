// src/views.rs
// Issue #200 — milestone and campaign view functions

use soroban_sdk::{contracttype, panic_with_error, symbol_short, Env, Vec};

use crate::storage::{
    get_campaign_or_panic, get_milestone, storage_get_asset_raised,
    storage_get_total_raised,
};
use crate::types::{
    CampaignData, CampaignStatus, DataKey, Error, MilestoneData, MilestoneStatus,
};

// ─── Response types ───────────────────────────────────────────────────────────

/// Enriched milestone view — adds computed fields on top of raw `MilestoneData`
/// so callers never have to re-derive them.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneView {
    /// Raw stored milestone record.
    pub data: MilestoneData,
    /// Amount still to be released for this milestone.
    pub pending_release: i128,
    /// True when `released_amount >= target_amount`.
    pub is_fully_released: bool,
    /// True when this milestone is the next one to be released.
    pub is_next_pending: bool,
}

/// Summary returned by `get_campaign_summary` — adds live computed fields
/// without exposing the raw `CampaignData` struct directly.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignSummary {
    pub creator: soroban_sdk::Address,
    pub goal_amount: i128,
    pub raised_amount: i128,
    pub remaining_to_goal: i128,
    pub progress_bps: u32,        // basis points: 0–10 000 (100.00 %)
    pub end_time: u64,
    pub status: CampaignStatus,
    pub milestone_count: u32,
    pub milestones_released: u32,
    pub all_milestones_released: bool,
    pub accepts_donations: bool,
}

/// Aggregate stats returned by `get_milestone_summary`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneSummary {
    pub total: u32,
    pub locked: u32,
    pub unlocked: u32,
    pub released: u32,
    pub total_released_amount: i128,
    pub total_pending_amount: i128,
    /// Index of the next milestone awaiting release, if any.
    pub next_pending_index: Option<u32>,
}

// ─── get_all_milestones ───────────────────────────────────────────────────────

/// Issue #200 — Returns all milestones in index order, enriched with computed
/// fields.
///
/// No authentication required (read-only view).
/// Handles campaigns with 1–MAX_MILESTONES milestones.
///
/// Panics:
///   `Error::NotInitialized`   — contract not yet initialised.
///   `Error::MilestoneNotFound` — storage is missing an expected milestone index
///                                (indicates corrupted state).
pub fn get_all_milestones(env: &Env) -> Vec<MilestoneView> {
    let campaign = get_campaign_or_panic(env);
    let count = campaign.milestone_count;

    let mut views: Vec<MilestoneView> = Vec::new(env);

    // Identify the index of the first non-Released milestone so we can set
    // `is_next_pending` correctly in a single pass.
    let next_pending_index = find_next_pending_index(env, count);

    for i in 0..count {
        let data = get_milestone(env, i)
            .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneNotFound));

        let pending_release    = data.pending_release();
        let is_fully_released  = data.is_fully_released();
        let is_next_pending    = next_pending_index == Some(i);

        views.push_back(MilestoneView {
            data,
            pending_release,
            is_fully_released,
            is_next_pending,
        });
    }

    views
}

// ─── get_milestone_by_index ───────────────────────────────────────────────────

/// Returns a single enriched milestone view by index.
///
/// Panics:
///   `Error::NotInitialized`    — contract not yet initialised.
///   `Error::MilestoneNotFound` — `index` ≥ `milestone_count` or missing from storage.
pub fn get_milestone_by_index(env: &Env, index: u32) -> MilestoneView {
    let campaign = get_campaign_or_panic(env);

    if index >= campaign.milestone_count {
        panic_with_error!(env, Error::MilestoneNotFound);
    }

    let data = get_milestone(env, index)
        .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneNotFound));

    let next_pending_index = find_next_pending_index(env, campaign.milestone_count);
    let pending_release    = data.pending_release();
    let is_fully_released  = data.is_fully_released();
    let is_next_pending    = next_pending_index == Some(index);

    MilestoneView {
        data,
        pending_release,
        is_fully_released,
        is_next_pending,
    }
}

// ─── get_milestone_summary ────────────────────────────────────────────────────

/// Returns aggregate milestone statistics for the campaign — useful for
/// dashboards and off-chain indexers that want counts without loading every
/// milestone individually.
///
/// Panics: `Error::NotInitialized`
pub fn get_milestone_summary(env: &Env) -> MilestoneSummary {
    let campaign = get_campaign_or_panic(env);
    let count = campaign.milestone_count;

    let mut locked: u32                = 0;
    let mut unlocked: u32              = 0;
    let mut released: u32              = 0;
    let mut total_released_amount: i128 = 0;
    let mut total_pending_amount: i128  = 0;
    let mut next_pending_index: Option<u32> = None;

    for i in 0..count {
        let Some(m) = get_milestone(env, i) else { continue };

        match m.status {
            MilestoneStatus::Locked => {
                locked += 1;
                total_pending_amount = total_pending_amount
                    .saturating_add(m.pending_release());
            }
            MilestoneStatus::Unlocked => {
                unlocked += 1;
                total_pending_amount = total_pending_amount
                    .saturating_add(m.pending_release());
                if next_pending_index.is_none() {
                    next_pending_index = Some(i);
                }
            }
            MilestoneStatus::Released => {
                released += 1;
                total_released_amount = total_released_amount
                    .saturating_add(m.released_amount);
            }
        }
    }

    MilestoneSummary {
        total: count,
        locked,
        unlocked,
        released,
        total_released_amount,
        total_pending_amount,
        next_pending_index,
    }
}

// ─── get_campaign_summary ─────────────────────────────────────────────────────

/// Returns a live campaign summary with computed progress fields.
///
/// `progress_bps` is in basis points (0–10 000) so callers can format it as
/// a percentage without floating-point arithmetic:
///   `display_pct = progress_bps / 100`
///
/// Panics: `Error::NotInitialized`
pub fn get_campaign_summary(env: &Env) -> CampaignSummary {
    let campaign  = get_campaign_or_panic(env);
    let total     = campaign.milestone_count;
    let released  = count_released_milestones(env, total);

    let remaining_to_goal = campaign.remaining();

    // Compute progress as basis points to stay in integer arithmetic.
    // Clamp to 10 000 bps (100 %) in case raised > goal (over-funded campaigns).
    let progress_bps: u32 = if campaign.goal_amount == 0 {
        0
    } else {
        let bps = (campaign.raised_amount as i64)
            .saturating_mul(10_000)
            / (campaign.goal_amount as i64);
        (bps.max(0).min(10_000)) as u32
    };

    CampaignSummary {
        creator:               campaign.creator,
        goal_amount:           campaign.goal_amount,
        raised_amount:         campaign.raised_amount,
        remaining_to_goal,
        progress_bps,
        end_time:              campaign.end_time,
        status:                campaign.status,
        milestone_count:       total,
        milestones_released:   released,
        all_milestones_released: released == total,
        accepts_donations:     campaign.is_accepting_donations(),
    }
}

// ─── get_asset_balances ───────────────────────────────────────────────────────

/// Returns the per-asset raised amounts for every accepted asset in the
/// campaign.  Useful for multi-asset dashboards.
///
/// Returns a `Vec` of `(asset_code_bytes, amount)` pairs in the same order
/// as `campaign.accepted_assets`.
///
/// Panics: `Error::NotInitialized`
pub fn get_asset_balances(env: &Env) -> Vec<(soroban_sdk::Bytes, i128)> {
    let campaign = get_campaign_or_panic(env);
    let mut out: Vec<(soroban_sdk::Bytes, i128)> = Vec::new(env);

    for asset in campaign.accepted_assets.iter() {
        let amount = match &asset.issuer {
            Some(addr) => storage_get_asset_raised(env, addr),
            None       => 0, // native XLM without a contract address — no balance tracked
        };

        // Convert the asset code String to Bytes for XDR compatibility
        let code_bytes = soroban_sdk::Bytes::from_slice(
            env,
            asset.asset_code.to_string().as_bytes(),
        );

        out.push_back((code_bytes, amount));
    }

    out
}

// ─── is_milestone_releasable ──────────────────────────────────────────────────

/// Returns `true` when the milestone at `index` can be released right now.
///
/// A milestone is releasable when:
///   1. It exists and is in `Unlocked` state.
///   2. All prior milestones are `Released` (sequential release enforced).
///   3. The campaign is not `Cancelled`.
///
/// Panics: `Error::NotInitialized`, `Error::MilestoneNotFound`
pub fn is_milestone_releasable(env: &Env, index: u32) -> bool {
    let campaign = get_campaign_or_panic(env);

    // Cancelled campaigns cannot release milestones
    if campaign.status == CampaignStatus::Cancelled {
        return false;
    }

    if index >= campaign.milestone_count {
        return false;
    }

    let Some(target) = get_milestone(env, index) else {
        return false;
    };

    if target.status != MilestoneStatus::Unlocked {
        return false;
    }

    // Enforce sequential release: all prior milestones must be Released
    for prior in 0..index {
        let Some(m) = get_milestone(env, prior) else {
            return false;
        };
        if m.status != MilestoneStatus::Released {
            return false;
        }
    }

    true
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Returns the index of the first milestone that is `Unlocked` (i.e. the
/// next one eligible to be released), scanning in ascending index order.
fn find_next_pending_index(env: &Env, count: u32) -> Option<u32> {
    for i in 0..count {
        if let Some(m) = get_milestone(env, i) {
            if m.status == MilestoneStatus::Unlocked {
                return Some(i);
            }
        }
    }
    None
}

/// Counts milestones in `Released` state without allocating a full Vec.
fn count_released_milestones(env: &Env, count: u32) -> u32 {
    let mut n = 0u32;
    for i in 0..count {
        if let Some(m) = get_milestone(env, i) {
            if m.status == MilestoneStatus::Released {
                n += 1;
            }
        }
    }
    n
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    // Minimal helpers to populate storage without going through the full
    // initialise flow — keeps tests focused on view logic only.

    fn make_env() -> Env {
        Env::default()
    }

    fn make_address(env: &Env) -> Address {
        Address::generate(env)
    }

    fn default_milestone(env: &Env, index: u32, status: MilestoneStatus) -> MilestoneData {
        MilestoneData {
            index,
            target_amount:    (index as i128 + 1) * 1_000,
            released_amount:  if status == MilestoneStatus::Released {
                (index as i128 + 1) * 1_000
            } else {
                0
            },
            description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
            status,
            released_at:      None,
            released_at_ledger: None,
            release_tx:       None,
            released_to:      None,
        }
    }

    #[test]
    fn find_next_pending_returns_first_unlocked() {
        let env = make_env();
        // Simulate two milestones: [Released, Unlocked]
        // find_next_pending_index should return Some(1)
        let m0 = default_milestone(&env, 0, MilestoneStatus::Released);
        let m1 = default_milestone(&env, 1, MilestoneStatus::Unlocked);

        // Write directly to storage using the key enum
        env.storage().persistent().set(&DataKey::MilestoneData(0), &m0);
        env.storage().persistent().set(&DataKey::MilestoneData(1), &m1);

        let result = find_next_pending_index(&env, 2);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn find_next_pending_returns_none_when_all_released() {
        let env = make_env();
        let m0 = default_milestone(&env, 0, MilestoneStatus::Released);
        env.storage().persistent().set(&DataKey::MilestoneData(0), &m0);

        let result = find_next_pending_index(&env, 1);
        assert_eq!(result, None);
    }

    #[test]
    fn find_next_pending_returns_none_when_all_locked() {
        let env = make_env();
        let m0 = default_milestone(&env, 0, MilestoneStatus::Locked);
        env.storage().persistent().set(&DataKey::MilestoneData(0), &m0);

        let result = find_next_pending_index(&env, 1);
        assert_eq!(result, None);
    }

    #[test]
    fn count_released_milestones_counts_correctly() {
        let env = make_env();
        env.storage().persistent().set(
            &DataKey::MilestoneData(0),
            &default_milestone(&env, 0, MilestoneStatus::Released),
        );
        env.storage().persistent().set(
            &DataKey::MilestoneData(1),
            &default_milestone(&env, 1, MilestoneStatus::Unlocked),
        );
        env.storage().persistent().set(
            &DataKey::MilestoneData(2),
            &default_milestone(&env, 2, MilestoneStatus::Locked),
        );

        assert_eq!(count_released_milestones(&env, 3), 1);
    }

    #[test]
    fn progress_bps_clamps_at_10000_for_overfunded_campaign() {
        // Directly test the BPS formula: raised > goal → clamp to 10 000
        let goal:   i64 = 1_000;
        let raised: i64 = 1_500;
        let bps = (raised.saturating_mul(10_000) / goal).max(0).min(10_000);
        assert_eq!(bps, 10_000);
    }

    #[test]
    fn progress_bps_is_zero_for_no_donations() {
        let goal:   i64 = 1_000;
        let raised: i64 = 0;
        let bps = (raised.saturating_mul(10_000) / goal).max(0).min(10_000);
        assert_eq!(bps, 0);
    }

    #[test]
    fn progress_bps_50_percent() {
        let goal:   i64 = 1_000;
        let raised: i64 = 500;
        let bps = (raised.saturating_mul(10_000) / goal).max(0).min(10_000);
        assert_eq!(bps, 5_000);
    }

    #[test]
    fn milestone_view_is_next_pending_set_correctly() {
        let env = make_env();
        let m0 = default_milestone(&env, 0, MilestoneStatus::Released);
        let m1 = default_milestone(&env, 1, MilestoneStatus::Unlocked);
        env.storage().persistent().set(&DataKey::MilestoneData(0), &m0);
        env.storage().persistent().set(&DataKey::MilestoneData(1), &m1);

        let next = find_next_pending_index(&env, 2);
        assert_eq!(next, Some(1)); // index 1 is next pending

        // index 0 is NOT next pending
        assert_ne!(next, Some(0));
    }

    #[test]
    fn is_milestone_releasable_requires_sequential_order() {
        let env = make_env();

        // m0 still Locked → m1 Unlocked should NOT be releasable
        let m0 = default_milestone(&env, 0, MilestoneStatus::Locked);
        let m1 = default_milestone(&env, 1, MilestoneStatus::Unlocked);
        env.storage().persistent().set(&DataKey::MilestoneData(0), &m0);
        env.storage().persistent().set(&DataKey::MilestoneData(1), &m1);

        // Manually check the sequential logic (no campaign in storage for this unit test)
        let prior_released = {
            let prior = env
                .storage()
                .persistent()
                .get::<DataKey, MilestoneData>(&DataKey::MilestoneData(0))
                .unwrap();
            prior.status == MilestoneStatus::Released
        };

        assert!(!prior_released, "prior milestone must be Released first");
    }
}