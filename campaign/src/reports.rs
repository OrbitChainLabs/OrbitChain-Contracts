//! Report and analytics helpers for the campaign contract.
//!
//! Provides campaign report building and active campaign counting. These are
//! extracted from the monolithic `lib.rs` into an independently mock-able module.

use soroban_sdk::Env;

use crate::types::Error;

/// Calculate the pro-rata refund amount with anti-dust floor.
pub(crate) fn calculate_refund_amount(
    env: &Env,
    donor_asset_amount: i128,
    refund_numerator: i128,
    refund_denominator: i128,
) -> i128 {
    if refund_denominator <= 0 {
        env.panic_with_error(Error::Overflow)
    }

    let numerator = donor_asset_amount
        .checked_mul(refund_numerator)
        .unwrap_or_else(|| env.panic_with_error(Error::Overflow));

    let refund = numerator / refund_denominator;

    if refund == 0 && numerator > 0 {
        1
    } else {
        refund
    }
}

use crate::storage::{
    get_campaign, storage_get_donation_count, storage_get_release_count,
    storage_get_unique_donor_count,
};
use crate::types::{CampaignData, CampaignReport};

/// Returns 1 if a campaign exists and is accepting donations, 0 otherwise.
pub(crate) fn active_campaign_count(env: &Env) -> u64 {
    match get_campaign(env) {
        Some(campaign) if campaign.status.accepts_donations() => 1,
        _ => 0,
    }
}

/// Build a dashboard-ready campaign report from storage data.
pub(crate) fn build_campaign_report(env: &Env, campaign: CampaignData) -> CampaignReport {
    let creator = campaign.creator.clone();
    let remaining_amount = campaign.remaining();
    let progress_bps = if campaign.goal_amount <= 0 || campaign.raised_amount <= 0 {
        0
    } else if campaign.raised_amount >= campaign.goal_amount {
        10_000
    } else {
        let scaled = campaign
            .raised_amount
            .checked_mul(10_000)
            .unwrap_or_else(|| env.panic_with_error(Error::Overflow));
        (scaled / campaign.goal_amount) as u32
    };

    CampaignReport {
        creator,
        goal_amount: campaign.goal_amount,
        raised_amount: campaign.raised_amount,
        remaining_amount,
        progress_bps,
        end_time: campaign.end_time,
        status: campaign.status,
        milestone_count: campaign.milestone_count,
        donor_count: storage_get_unique_donor_count(env),
        donation_count: storage_get_donation_count(env),
        release_count: storage_get_release_count(env),
    }
}
