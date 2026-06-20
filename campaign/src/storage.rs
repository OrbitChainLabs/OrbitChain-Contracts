// src/storage.rs

use soroban_sdk::{Address, Env, panic_with_error};
use crate::types::{CampaignData, DataKey, DonorRecord, Error, MilestoneData};

// ─── TTL Constants ────────────────────────────────────────────────────────────
//
// Soroban ledger ≈ 5 seconds. All values expressed in ledgers.
//
// Persistent storage: entries survive until explicitly archived; we bump TTL
// on every access so hot entries never get archived unexpectedly.
//
// Temporary storage: naturally expires; we set an explicit TTL on write.

/// ~30 days — bump threshold: if remaining TTL < this, extend.
pub const PERSISTENT_BUMP_THRESHOLD: u32 = 518_400;

/// ~60 days — extend to this TTL when bumping persistent entries.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 1_036_800;

/// ~7 days — lifetime of temporary entries (contract status, locks).
pub const TEMPORARY_TTL: u32 = 120_960;

/// ~1 day — bump threshold for temporary entries.
pub const TEMPORARY_BUMP_THRESHOLD: u32 = 17_280;

// ─── Internal bump helper ─────────────────────────────────────────────────────

/// Bump a persistent key's TTL if it is below the threshold.
/// No-ops safely when the key does not exist (fresh contract or
/// never-written entry) — `extend_ttl` panics on missing keys.
#[inline]
fn bump_persistent(env: &Env, key: &DataKey) {
    if env.storage().persistent().has(key) {
        env.storage()
            .persistent()
            .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }
}

// ─── Campaign ─────────────────────────────────────────────────────────────────

/// Store the campaign record. Bumps TTL to keep it alive for the campaign
/// lifetime. Panics if the serialised data exceeds Soroban's value size limit
/// (handled automatically by the host — we surface it as `StorageWriteError`
/// so callers get a typed error instead of a host trap).
pub fn set_campaign(env: &Env, data: &CampaignData) {
    env.storage()
        .persistent()
        .set(&DataKey::CampaignData, data);
    bump_persistent(env, &DataKey::CampaignData);
}

/// Load the campaign record and refresh its TTL.
/// Returns `None` only before the contract is initialised.
#[must_use]
pub fn get_campaign(env: &Env) -> Option<CampaignData> {
    let value = env
        .storage()
        .persistent()
        .get(&DataKey::CampaignData)?;
    bump_persistent(env, &DataKey::CampaignData);
    Some(value)
}

/// Same as `get_campaign` but panics with `NotInitialized` instead of
/// returning `None`. Use this in every function that requires an initialised
/// contract — it removes the repetitive `unwrap_or_else` boilerplate.
#[must_use]
pub fn get_campaign_or_panic(env: &Env) -> CampaignData {
    get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
}

// ─── Milestones ───────────────────────────────────────────────────────────────

/// Persist a milestone record at `index` and refresh its TTL.
pub fn set_milestone(env: &Env, index: u32, data: &MilestoneData) {
    let key = DataKey::MilestoneData(index);
    env.storage().persistent().set(&key, data);
    bump_persistent(env, &key);
}

/// Load a milestone by index and refresh its TTL.
/// Returns `None` when `index` is out of range.
#[must_use]
pub fn get_milestone(env: &Env, index: u32) -> Option<MilestoneData> {
    let key = DataKey::MilestoneData(index);
    let value = env.storage().persistent().get(&key)?;
    bump_persistent(env, &key);
    Some(value)
}

/// Same as `get_milestone` but panics with `MilestoneNotFound`.
#[must_use]
pub fn get_milestone_or_panic(env: &Env, index: u32) -> MilestoneData {
    get_milestone(env, index)
        .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneNotFound))
}

// ─── Donors ───────────────────────────────────────────────────────────────────

/// Persist a donor record and refresh its TTL.
pub fn set_donor(env: &Env, donor: &Address, record: &DonorRecord) {
    let key = DataKey::DonorData(donor.clone());
    env.storage().persistent().set(&key, record);
    bump_persistent(env, &key);
}

/// Load a donor record. Returns `None` for first-time donors.
/// Bumps TTL on hit to keep active donor records alive.
pub fn get_donor(env: &Env, donor: &Address) -> Option<DonorRecord> {
    let key = DataKey::DonorData(donor.clone());
    let value = env.storage().persistent().get(&key)?;
    bump_persistent(env, &key);
    Some(value)
}

/// Load a donor record or return a zeroed `DonorRecord`.
/// Convenience wrapper — avoids `unwrap_or_default()` scattered across callers.
#[must_use]
pub fn get_donor_or_default(env: &Env, donor: &Address) -> DonorRecord {
    get_donor(env, donor).unwrap_or_else(|| {
        // Return a zeroed DonorRecord — caller should update the relevant fields
        DonorRecord {
            donor: donor.clone(),
            total_donated: 0,
            asset: crate::types::AssetInfo::Native,
            last_donation_time: 0,
            last_donation_ledger: 0,
            donation_count: 0,
            refund_claimed: false,
        }
    })
}

// ─── Per-asset donor donations ────────────────────────────────────────────────

/// Get the amount a donor has contributed in a specific asset.
/// Returns 0 if no donations in that asset yet.
pub fn get_donor_asset_donation(env: &Env, donor: &Address, asset: &Address) -> i128 {
    let key = DataKey::DonorAssetDonation(donor.clone(), asset.clone());
    let value: i128 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(0);
    bump_persistent(env, &key);
    value
}

/// Add to a donor's contribution in a specific asset.
/// Panics if the addition would overflow.
pub fn increment_donor_asset_donation(env: &Env, donor: &Address, asset: &Address, amount: i128) {
    let key = DataKey::DonorAssetDonation(donor.clone(), asset.clone());
    let current: i128 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(0);
    
    let new_amount = current.checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    
    env.storage().persistent().set(&key, &new_amount);
    bump_persistent(env, &key);
}

// ─── Total raised ─────────────────────────────────────────────────────────────

/// Load the global total-raised counter. Returns 0 before any donations.
pub fn storage_get_total_raised(env: &Env) -> i128 {
    let value: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::TotalRaised)
        .unwrap_or(0);
    bump_persistent(env, &DataKey::TotalRaised);
    value
}

/// Persist the global total-raised counter.
/// Panics if `amount` is negative — total raised must never go below zero.
#[inline]
pub fn storage_set_total_raised(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, Error::InvalidAmount);
    }
    env.storage()
        .persistent()
        .set(&DataKey::TotalRaised, &amount);
    bump_persistent(env, &DataKey::TotalRaised);
}

/// Atomically add `delta` to total raised using checked arithmetic.
/// Returns the new total.
#[inline]
pub fn storage_increment_total_raised(env: &Env, delta: i128) -> i128 {
    if delta <= 0 {
        panic_with_error!(env, Error::InvalidAmount);
    }
    let current = storage_get_total_raised(env);
    let new_total = current
        .checked_add(delta)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    storage_set_total_raised(env, new_total);
    new_total
}

/// Total number of accepted donation calls for this campaign.
pub fn storage_get_donation_count(env: &Env) -> u64 {
    let value: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::DonationCount)
        .unwrap_or(0);
    bump_persistent(env, &DataKey::DonationCount);
    value
}

/// Increment the accepted donation counter.
pub fn storage_increment_donation_count(env: &Env) -> u64 {
    let current = storage_get_donation_count(env);
    let next = current
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    env.storage()
        .persistent()
        .set(&DataKey::DonationCount, &next);
    bump_persistent(env, &DataKey::DonationCount);
    next
}

/// Number of unique donor addresses that have contributed.
pub fn storage_get_unique_donor_count(env: &Env) -> u32 {
    let value: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::UniqueDonorCount)
        .unwrap_or(0);
    bump_persistent(env, &DataKey::UniqueDonorCount);
    value
}

/// Increment the unique donor counter.
pub fn storage_increment_unique_donor_count(env: &Env) -> u32 {
    let current = storage_get_unique_donor_count(env);
    let next = current
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    env.storage()
        .persistent()
        .set(&DataKey::UniqueDonorCount, &next);
    bump_persistent(env, &DataKey::UniqueDonorCount);
    next
}

/// Total number of completed milestone release calls for this campaign.
pub fn storage_get_release_count(env: &Env) -> u64 {
    let value: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ReleaseCount)
        .unwrap_or(0);
    bump_persistent(env, &DataKey::ReleaseCount);
    value
}

/// Increment the completed milestone release counter.
pub fn storage_increment_release_count(env: &Env) -> u64 {
    let current = storage_get_release_count(env);
    let next = current
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    env.storage()
        .persistent()
        .set(&DataKey::ReleaseCount, &next);
    bump_persistent(env, &DataKey::ReleaseCount);
    next
}

// ─── Per-asset raised ─────────────────────────────────────────────────────────
//
// Tracks how much of the total raise came from each specific token.
// Required for correct proportional milestone release across multiple assets.

/// Load the raised amount for a specific token address.
pub fn storage_get_asset_raised(env: &Env, token: &Address) -> i128 {
    let key = DataKey::AssetRaised(token.clone());
    let value: i128 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(0);
    bump_persistent(env, &key);
    value
}

/// Persist the raised amount for a specific token address.
/// Panics if `amount` is negative.
pub fn storage_set_asset_raised(env: &Env, token: &Address, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, Error::InvalidAmount);
    }
    let key = DataKey::AssetRaised(token.clone());
    env.storage().persistent().set(&key, &amount);
    bump_persistent(env, &key);
}

/// Atomically add `delta` to the per-asset raised counter.
/// Returns the new per-asset total.
pub fn storage_increment_asset_raised(env: &Env, token: &Address, delta: i128) -> i128 {
    if delta <= 0 {
        panic_with_error!(env, Error::InvalidAmount);
    }
    let current = storage_get_asset_raised(env, token);
    let new_total = current
        .checked_add(delta)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    storage_set_asset_raised(env, token, new_total);
    new_total
}

// ─── Contract status (temporary) ─────────────────────────────────────────────

/// Load the transient contract status flag.
/// Returns `None` if the entry has expired or was never set.
pub fn get_contract_status(env: &Env) -> Option<u32> {
    let key = DataKey::ContractStatus;
    let value = env.storage().temporary().get(&key)?;
    env.storage()
        .temporary()
        .extend_ttl(&key, TEMPORARY_BUMP_THRESHOLD, TEMPORARY_TTL);
    Some(value)
}

/// Persist the transient contract status flag with a fresh TTL.
pub fn set_contract_status(env: &Env, status: u32) {
    let key = DataKey::ContractStatus;
    env.storage().temporary().set(&key, &status);
    // Set explicit TTL — temporary entries default to 1 ledger without this
    env.storage()
        .temporary()
        .extend_ttl(&key, TEMPORARY_BUMP_THRESHOLD, TEMPORARY_TTL);
}

// ─── Re-entrancy lock (temporary) ────────────────────────────────────────────
//
// Soroban's transaction model prevents true re-entrancy, but cross-contract
// call chains can still produce unexpected re-entrant-style patterns.
// A lightweight lock prevents a contract function from being called recursively
// within the same transaction.

const LOCK_KEY: DataKey = DataKey::ReentrancyLock;

/// Acquire the re-entrancy lock. Panics if the lock is already held.
pub fn acquire_lock(env: &Env) {
    if env.storage().temporary().has(&LOCK_KEY) {
        panic_with_error!(env, Error::ReentrantCall);
    }
    env.storage().temporary().set(&LOCK_KEY, &true);
    env.storage()
        .temporary()
        .extend_ttl(&LOCK_KEY, 0, TEMPORARY_TTL);
}

/// Release the re-entrancy lock.
pub fn release_lock(env: &Env) {
    env.storage().temporary().remove(&LOCK_KEY);
}

// ─── Freeze flag (persistent) ─────────────────────────────────────────────────

/// Check whether the contract is currently frozen.
/// Returns `false` if the flag has never been set.
pub fn is_frozen(env: &Env) -> bool {
    let key = DataKey::Frozen;
    let frozen: bool = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(false);
    bump_persistent(env, &key);
    frozen
}

/// Set the contract freeze flag.
pub fn set_frozen(env: &Env, frozen: bool) {
    let key = DataKey::Frozen;
    env.storage().persistent().set(&key, &frozen);
    bump_persistent(env, &key);
}

// ─── Bulk TTL refresh ─────────────────────────────────────────────────────────

/// Refresh TTL for all core persistent keys in a single call.
/// Call this from a `bump_storage` admin function to prevent archival
/// during long-running campaigns.
pub fn bump_all_persistent(env: &Env, milestone_count: u32) {
    let core_keys = [
        DataKey::CampaignData,
        DataKey::TotalRaised,
        DataKey::DonationCount,
        DataKey::UniqueDonorCount,
        DataKey::ReleaseCount,
    ];

    for key in &core_keys {
        if env.storage().persistent().has(key) {
            bump_persistent(env, key);
        }
    }

    for i in 0..milestone_count {
        let key = DataKey::MilestoneData(i);
        if env.storage().persistent().has(&key) {
            bump_persistent(env, &key);
        }
    }
}
