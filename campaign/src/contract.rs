//! Campaign lifecycle management functions (end, cancel, extend deadline).
//!
//! These are wired into the contract impl in `lib.rs` as methods on
//! `CampaignContract`.

use crate::event;
use crate::storage::{get_campaign, is_frozen, set_campaign};
use crate::types::{CampaignStatus, Error};
use crate::{validate_campaign_transition, MAX_DEADLINE_GAP_SECONDS};
use soroban_sdk::{panic_with_error, Env};

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

/// Issue #45 — Get campaign hours remaining until deadline.
///
/// Returns the hours portion of the remaining time (0 to 23) in the current day before the deadline.
/// If the deadline has passed, returns 0.
/// No auth required (read-only view).
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
#[must_use]
pub fn get_campaign_hours_remaining(env: &Env) -> u32 {
    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    let now = env.ledger().timestamp();
    if now < campaign.end_time {
        let delta = campaign.end_time - now;
        ((delta % 86_400) / 3_600) as u32
    } else {
        0
    }
}
