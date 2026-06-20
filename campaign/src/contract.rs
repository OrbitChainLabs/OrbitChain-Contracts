//! Campaign lifecycle management functions (end, cancel, extend deadline).
//!
//! These are wired into the contract impl in `lib.rs` as methods on
//! `CampaignContract`.

use soroban_sdk::{panic_with_error, Address, Env};
use crate::event;
use crate::storage::{get_campaign, set_campaign};
use crate::types::{CampaignStatus, Error};
use crate::validate_campaign_transition;

/// Issue #212 – End the campaign early (before deadline).
///
/// Transitions the campaign from `Active` or `GoalReached` to `Ended`.
/// Requires creator authorization.
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
/// - `Error::Unauthorized` if caller is not the creator
/// - `Error::InvalidCampaignTransition` if campaign is already Ended or Cancelled
pub fn end_campaign(env: &Env) {
    let mut campaign = get_campaign(env)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    campaign.creator.require_auth();

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
/// - `Error::InvalidCampaignTransition` if campaign is already Cancelled
pub fn cancel_campaign(env: &Env) {
    let mut campaign = get_campaign(env)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    campaign.creator.require_auth();

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
/// Requires creator authorization.
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
/// - `Error::Unauthorized` if caller is not the creator
/// - `Error::InvalidEndTime` if `new_end_time <= current ledger timestamp`
/// - `Error::InvalidCampaignTransition` if campaign is not Active or GoalReached
pub fn extend_deadline(env: &Env, new_end_time: u64) {
    let mut campaign = get_campaign(env)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    campaign.creator.require_auth();

    match campaign.status {
        CampaignStatus::Active | CampaignStatus::GoalReached => {}
        _ => panic_with_error!(env, Error::InvalidCampaignTransition),
    }

    let current_time = env.ledger().timestamp();
    if new_end_time <= current_time {
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
    
    let campaign = get_campaign(env)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

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
