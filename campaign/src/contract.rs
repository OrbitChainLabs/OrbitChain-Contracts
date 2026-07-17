//! Campaign lifecycle management functions (end, cancel, extend deadline).
//!
//! These are wired into the contract impl in `lib.rs` as methods on
//! `CampaignContract`.

use crate::event;
use crate::storage::{get_campaign, is_frozen, set_campaign, set_wrapped_native_xlm};
use crate::types::{CampaignStatus, Error, StellarAsset};
use crate::{validate_campaign_transition, MAX_DEADLINE_GAP_SECONDS};
use soroban_sdk::{panic_with_error, Address, Bytes, Env, String, Vec};

/// Exact asset code for native XLM. Matching is equality-only — codes such as
/// `"XLMUSD"` must never be treated as native XLM.
const NATIVE_XLM_CODE: &str = "XLM";

/// Asset::Native XDR (AssetType::Native = 0). Used with
/// `env.deployer().with_stellar_asset(...)` to derive the deterministic
/// wrapped-XLM Stellar Asset Contract address.
const ASSET_NATIVE_XDR: [u8; 4] = [0, 0, 0, 0];

/// Resolve `AssetInfo::Native` → wrapped XLM SEP-41 contract address and cache
/// it under `DataKey::WrappedNativeXlm`.
///
/// Resolution order (scanned once at initialize, never on the donate hot path):
/// 1. Exact `asset_code == "XLM"` with `issuer: None` → host-derived SAC id via
///    `env.deployer().with_stellar_asset(Asset::Native)`.
/// 2. Exact `asset_code == "XLM"` with `issuer: Some(addr)` → use `addr` as the
///    pre-wrapped SAC address (compatible with existing campaign configs).
///
/// No-ops when the campaign does not accept XLM.
pub fn resolve_and_cache_wrapped_native_xlm(env: &Env, accepted_assets: &Vec<StellarAsset>) {
    if let Some(address) = resolve_wrapped_native_xlm(env, accepted_assets) {
        set_wrapped_native_xlm(env, &address);
    }
}

/// Env-aware wrapper that maps native XLM to its wrapped SAC address.
///
/// Uses the current contract's host/deployer context (`env.deployer()`) so the
/// address is network-correct without a linear scan of `accepted_assets`.
pub fn resolve_wrapped_native_xlm(
    env: &Env,
    accepted_assets: &Vec<StellarAsset>,
) -> Option<Address> {
    let xlm_code = String::from_str(env, NATIVE_XLM_CODE);

    // Canonical native entry: exact "XLM", no issuer → derive SAC from Asset::Native.
    if accepted_assets
        .iter()
        .any(|a| a.asset_code == xlm_code && a.issuer.is_none())
    {
        return Some(wrapped_native_xlm_sac_address(env));
    }

    // Compat: exact "XLM" with issuer holding the wrapped SAC address.
    accepted_assets
        .iter()
        .find(|a| a.asset_code == xlm_code)
        .and_then(|a| a.issuer.clone())
}

/// Derive the deterministic wrapped-native-XLM contract address via the host.
fn wrapped_native_xlm_sac_address(env: &Env) -> Address {
    let serialized = Bytes::from_array(env, &ASSET_NATIVE_XDR);
    env.deployer()
        .with_stellar_asset(serialized)
        .deployed_address()
}

/// Issue #212 – End the campaign early (before deadline).
///
/// Transitions the campaign from `Active` or `GoalReached` to `Ended`.
/// Requires creator authorization.
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
/// - `Error::Unauthorized` if caller is not the creator
/// - `Error::ContractFrozen` if contract is frozen (freeze invariant: all writes rejected)
/// - `Error::InvalidCampaignTransition` if campaign is already Ended or Cancelled
pub fn end_campaign(env: &Env) {
    let mut campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    campaign.creator.require_auth();

    // Freeze invariant: all write operations are rejected while frozen (see freeze()).
    if is_frozen(env) {
        panic_with_error!(env, Error::ContractFrozen);
    }

    validate_campaign_transition(env, &campaign.status, &CampaignStatus::Ended)
        .unwrap_or_else(|e| panic_with_error!(env, e));

    campaign.status = CampaignStatus::Ended;
    campaign.concluded_at_ledger = Some(env.ledger().sequence());
    set_campaign(env, &campaign);

    event::campaign_ended(env);
}

/// Issue #214 – Cancel the campaign.
///
/// Transitions the campaign from `Active`, `GoalReached`, or `Ended` to
/// `Cancelled`.  Requires creator authorization.
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
/// - `Error::Unauthorized` if caller is not the creator
/// - `Error::ContractFrozen` if contract is frozen (freeze invariant: all writes rejected)
/// - `Error::InvalidCampaignTransition` if campaign is already Cancelled
pub fn cancel_campaign(env: &Env) {
    let mut campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    campaign.creator.require_auth();

    // Freeze invariant: all write operations are rejected while frozen (see freeze()).
    if is_frozen(env) {
        panic_with_error!(env, Error::ContractFrozen);
    }

    validate_campaign_transition(env, &campaign.status, &CampaignStatus::Cancelled)
        .unwrap_or_else(|e| panic_with_error!(env, e));

    campaign.status = CampaignStatus::Cancelled;
    campaign.concluded_at_ledger = Some(env.ledger().sequence());
    set_campaign(env, &campaign);

    event::campaign_cancelled(env, &campaign.creator);
}

/// Issue #215 – Extend the campaign deadline.
///
/// Extends the campaign's `end_time` to a new future timestamp.
/// The new deadline cannot be more than ten years from the current ledger time;
/// this preserves the contract's time arithmetic invariants for status views,
/// refund windows, milestone release metadata, and campaign reports.
/// Requires creator authorization.
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
/// - `Error::Unauthorized` if caller is not the creator
/// - `Error::ContractFrozen` if contract is frozen (freeze invariant: all writes rejected)
/// - `Error::InvalidEndTime` if `new_end_time <= current ledger timestamp`
/// - `Error::InvalidEndTime` if `new_end_time` is more than ten years out
/// - `Error::InvalidCampaignTransition` if campaign is not Active or GoalReached
pub fn extend_deadline(env: &Env, new_end_time: u64) {
    let mut campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    campaign.creator.require_auth();

    // Freeze invariant: all write operations are rejected while frozen (see freeze()).
    if is_frozen(env) {
        panic_with_error!(env, Error::ContractFrozen);
    }

    match campaign.status {
        CampaignStatus::Active | CampaignStatus::GoalReached => {}
        _ => panic_with_error!(env, Error::InvalidCampaignTransition),
    }

    let current_time = env.ledger().timestamp();
    let max_end_time = current_time.saturating_add(MAX_DEADLINE_GAP_SECONDS);
    if new_end_time <= current_time || new_end_time > max_end_time {
        panic_with_error!(env, Error::InvalidEndTime);
    }

    let old_deadline = campaign.end_time;
    campaign.end_time = new_end_time;
    set_campaign(env, &campaign);

    event::deadline_extended(env, &campaign.creator, old_deadline, new_end_time);
}

/// Issue #235 — Get campaign status with computed fields.
///
/// Returns the current `CampaignStatus` and `days_remaining` until deadline.
/// Negative `days_remaining` means the deadline has passed.
/// No auth required (read-only view).
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
#[must_use]
pub fn get_campaign_status(env: &Env) -> crate::types::CampaignStatusResponse {
    use crate::types::CampaignStatusResponse;

    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    let now = env.ledger().timestamp();
    let days_remaining = if now < campaign.end_time {
        ((campaign.end_time - now) / 86_400) as i64
    } else {
        -(((now - campaign.end_time) / 86_400) as i64)
    };

    CampaignStatusResponse {
        status: campaign.status,
        days_remaining,
    }
}
