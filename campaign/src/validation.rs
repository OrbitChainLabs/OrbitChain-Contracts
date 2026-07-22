//! Input validation and transition helpers for the campaign contract.
//!
//! Provides asset validation, milestone validation, campaign status transition
//! validation, and milestone status transition validation. These are extracted
//! from the monolithic `lib.rs` into an independently mock-able module.

use soroban_sdk::{Address, Env, String, Vec};

use crate::storage::get_campaign;
use crate::types::{
    AssetInfo, CampaignData, CampaignStatus, Error, MilestoneData, MilestoneStatus, StellarAsset,
};
use crate::REFUND_WINDOW;

#[allow(dead_code)]
/// Issue #175 – assert the current invoker is the campaign creator.
pub(crate) fn require_creator(env: &Env) {
    let campaign = get_campaign(env).unwrap_or_else(|| env.panic_with_error(Error::Unauthorized));
    campaign.creator.require_auth();
}

pub(crate) fn get_token_address_for_asset(
    env: &Env,
    asset: &AssetInfo,
    campaign: &CampaignData,
) -> Address {
    match asset {
        AssetInfo::Stellar(addr) => {
            let accepted = campaign
                .accepted_assets
                .iter()
                .any(|a| a.issuer == Some(addr.clone()));
            if !accepted {
                env.panic_with_error(Error::AssetNotAccepted)
            }
            addr.clone()
        }
        AssetInfo::Native => {
            let xlm_code = soroban_sdk::String::from_str(env, "XLM");
            campaign
                .accepted_assets
                .iter()
                .find(|a| a.asset_code == xlm_code)
                .and_then(|a| a.issuer.clone())
                .unwrap_or_else(|| env.panic_with_error(Error::AssetNotAccepted))
        }
    }
}

pub(crate) fn resolve_asset_code(env: &Env, asset: &AssetInfo, campaign: &CampaignData) -> String {
    match asset {
        AssetInfo::Native => String::from_str(env, "XLM"),
        AssetInfo::Stellar(addr) => campaign
            .accepted_assets
            .iter()
            .find(|a| a.issuer == Some(addr.clone()))
            .map(|a| a.asset_code.clone())
            .unwrap_or_else(|| String::from_str(env, "UNKNOWN")),
    }
}

pub(crate) fn validate_assets(env: &Env, assets: &Vec<StellarAsset>) -> Result<(), Error> {
    for asset in assets.iter() {
        if asset.asset_code.is_empty() {
            env.panic_with_error(Error::InvalidAssetCode)
        }
    }
    Ok(())
}

pub(crate) fn validate_milestones(
    env: &Env,
    milestones: &Vec<MilestoneData>,
    goal_amount: i128,
) -> Result<(), Error> {
    for i in 1..milestones.len() {
        let prev = &milestones.get(i - 1).unwrap();
        let current = &milestones.get(i).unwrap();

        if prev.target_amount >= current.target_amount {
            env.panic_with_error(Error::InvalidMilestones)
        }
    }

    if let Some(last_milestone) = milestones.last() {
        if last_milestone.target_amount != goal_amount {
            env.panic_with_error(Error::MilestoneMismatch)
        }
    } else {
        env.panic_with_error(Error::InvalidMilestones)
    }

    Ok(())
}

pub(crate) fn validate_campaign_transition(
    _env: &Env,
    current_status: &CampaignStatus,
    next_status: &CampaignStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        (CampaignStatus::Active, CampaignStatus::GoalReached)
        | (CampaignStatus::Active, CampaignStatus::Ended)
        | (CampaignStatus::Active, CampaignStatus::Cancelled)
        | (CampaignStatus::GoalReached, CampaignStatus::Ended)
        | (CampaignStatus::GoalReached, CampaignStatus::Cancelled)
        | (CampaignStatus::Ended, CampaignStatus::Cancelled) => Ok(()),
        _ => Err(Error::InvalidCampaignTransition),
    }
}

#[allow(dead_code)]
pub(crate) fn validate_milestone_transition(
    _env: &Env,
    current_status: &MilestoneStatus,
    next_status: &MilestoneStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        (MilestoneStatus::Locked, MilestoneStatus::Unlocked)
        | (MilestoneStatus::Locked, MilestoneStatus::Released)
        | (MilestoneStatus::Unlocked, MilestoneStatus::Released) => Ok(()),
        _ => Err(Error::InvalidMilestoneTransition),
    }
}

pub(crate) fn check_refund_eligibility(
    env: &Env,
    campaign: &CampaignData,
    donor_record: &crate::types::DonorRecord,
) -> Result<(), Error> {
    if !campaign.status.is_terminal() {
        return Err(Error::RefundNotPermitted);
    }

    match campaign.status {
        CampaignStatus::Cancelled => {}
        CampaignStatus::Ended => {
            for i in 0..campaign.milestone_count {
                if let Some(milestone) = crate::storage::get_milestone(env, i) {
                    if milestone.status == MilestoneStatus::Released {
                        return Err(Error::RefundNotPermitted);
                    }
                }
            }
        }
        _ => return Err(Error::RefundNotPermitted),
    }

    let current_time = env.ledger().timestamp();
    if current_time > campaign.end_time + REFUND_WINDOW {
        return Err(Error::RefundWindowClosed);
    }

    if donor_record.refund_claimed {
        return Err(Error::RefundAlreadyClaimed);
    }

    Ok(())
}
