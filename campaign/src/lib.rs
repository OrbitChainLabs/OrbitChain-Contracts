#![no_std]

pub mod storage;
pub mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};
use types::{
    AssetInfo, CampaignData, CampaignInitializedEvent, CampaignStatus, DonorRecord, Error,
    MilestoneData, MilestoneStatus, StellarAsset,
};
use storage::{get_campaign, get_donor, set_campaign, set_donor, set_milestone};

pub const VERSION: u32 = 1;

#[contract]
pub struct CampaignContract;

#[contractimpl]
impl CampaignContract {
    /// Initialize a new campaign with strict validation on all inputs.
    ///
    /// Requires: Creator authorization via `creator.require_auth()`
    /// Can only be called once per contract instance
    ///
    /// # Panics
    /// - `Error::UnauthorizedCreator`   if caller is not the creator
    /// - `Error::AlreadyInitialized`    if campaign already exists
    /// - `Error::InvalidGoalAmount`     if goal_amount <= 0
    /// - `Error::InvalidEndTime`        if end_time <= current ledger timestamp
    /// - `Error::InvalidAssets`         if accepted_assets is empty
    /// - `Error::InvalidAssetCode`      if any asset_code is empty
    /// - `Error::InvalidMilestoneCount` if milestone count is not 1-5
    /// - `Error::InvalidMilestones`     if milestones are not sorted ascending
    /// - `Error::MilestoneMismatch`     if last milestone.target_amount != goal_amount
    pub fn initialize(
        env: Env,
        creator: soroban_sdk::Address,
        goal_amount: i128,
        end_time: u64,
        accepted_assets: Vec<StellarAsset>,
        milestones: Vec<MilestoneData>,
    ) -> Result<(), Error> {
        creator.require_auth();

        if get_campaign(&env).is_some() {
            panic_with_error(&env, Error::AlreadyInitialized);
        }

        if goal_amount <= 0 {
            panic_with_error(&env, Error::InvalidGoalAmount);
        }

        let current_timestamp = env.ledger().timestamp();
        if end_time <= current_timestamp {
            panic_with_error(&env, Error::InvalidEndTime);
        }

        if accepted_assets.is_empty() {
            panic_with_error(&env, Error::InvalidAssets);
        }

        validate_assets(&env, &accepted_assets)?;

        let milestone_count = milestones.len() as u32;
        if milestone_count == 0 || milestone_count > types::MAX_MILESTONES {
            panic_with_error(&env, Error::InvalidMilestoneCount);
        }

        validate_milestones(&env, &milestones, goal_amount)?;

        let campaign = CampaignData {
            creator: creator.clone(),
            goal_amount,
            raised_amount: 0,
            end_time,
            status: CampaignStatus::Active,
            accepted_assets: accepted_assets.clone(),
            milestone_count,
        };

        set_campaign(&env, &campaign);

        for (index, milestone) in milestones.iter().enumerate() {
            set_milestone(&env, index as u32, &milestone);
        }

        env.events().publish(
            ("campaign", "initialized"),
            CampaignInitializedEvent {
                creator,
                goal_amount,
                end_time,
                asset_count: accepted_assets.len() as u32,
                milestone_count,
            },
        );

        Ok(())
    }

    /// Issue #174 – read-only view of full campaign state; no auth required.
    pub fn get_campaign_info(env: Env) -> CampaignData {
        get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized))
    }

    /// Issue #176 + #177 – accept a donation in native XLM or any SEP-41 token.
    ///
    /// For `AssetInfo::Native` (XLM): the XLM entry in `accepted_assets` must
    /// carry the wrapped native token contract address in `StellarAsset.issuer`.
    /// For `AssetInfo::Stellar(addr)`: `addr` is the token contract address.
    ///
    /// # Panics
    /// - `Error::InvalidDonationAmount` if amount <= 0
    /// - `Error::NotInitialized`        if campaign not yet initialized
    /// - `Error::CampaignNotActive`     if status is not Active or GoalReached
    /// - `Error::CampaignEnded`         if current timestamp > campaign end_time
    /// - `Error::AssetNotAccepted`      if asset not in accepted_assets
    pub fn donate(env: Env, donor: Address, amount: i128, asset: AssetInfo) {
        donor.require_auth();

        if amount <= 0 {
            panic_with_error(&env, Error::InvalidDonationAmount);
        }

        let mut campaign =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        if campaign.status != CampaignStatus::Active
            && campaign.status != CampaignStatus::GoalReached
        {
            panic_with_error(&env, Error::CampaignNotActive);
        }

        let now = env.ledger().timestamp();
        if now > campaign.end_time {
            panic_with_error(&env, Error::CampaignEnded);
        }

        let token_addr = get_token_address_for_asset(&env, &asset, &campaign);

        token::Client::new(&env, &token_addr).transfer(
            &donor,
            &env.current_contract_address(),
            &amount,
        );

        campaign.raised_amount += amount;
        set_campaign(&env, &campaign);

        let record = match get_donor(&env, &donor) {
            Some(mut existing) => {
                existing.total_donated += amount;
                existing.last_donation_time = now;
                existing
            }
            None => DonorRecord {
                donor: donor.clone(),
                total_donated: amount,
                asset: asset.clone(),
                last_donation_time: now,
            },
        };
        set_donor(&env, &donor, &record);

        env.events()
            .publish(("donation", "received"), (donor, amount, asset));
    }

    pub fn hello(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "campaign")
    }

    pub fn version() -> u32 {
        VERSION
    }
}

/// Issue #175 – assert the current invoker is the campaign creator.
///
/// Reads the creator address from campaign storage and calls `require_auth()`.
/// Panics with `Error::UnauthorizedCreator` if the campaign is not initialized;
/// Soroban's auth framework panics if the invoker is not the creator.
fn require_creator(env: &Env) {
    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error(env, Error::UnauthorizedCreator));
    campaign.creator.require_auth();
}

/// Validates that `asset` is in the campaign's accepted list and returns the
/// token contract address needed to construct a `token::Client`.
///
/// - `AssetInfo::Stellar(addr)` → `addr` must match an accepted asset's issuer.
/// - `AssetInfo::Native` (XLM) → finds the XLM entry by asset_code and uses its issuer.
fn get_token_address_for_asset(
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
                panic_with_error(env, Error::AssetNotAccepted);
            }
            addr.clone()
        }
        AssetInfo::Native => {
            // Find the XLM entry in accepted_assets by asset_code == "XLM".
            // Its issuer must hold the wrapped native token contract address.
            let xlm_code = soroban_sdk::String::from_str(env, "XLM");
            campaign
                .accepted_assets
                .iter()
                .find(|a| a.asset_code == xlm_code)
                .and_then(|a| a.issuer.clone())
                .unwrap_or_else(|| panic_with_error(env, Error::AssetNotAccepted))
        }
    }
}

fn validate_assets(env: &Env, assets: &Vec<StellarAsset>) -> Result<(), Error> {
    for asset in assets.iter() {
        if asset.asset_code.len() == 0 {
            panic_with_error(env, Error::InvalidAssetCode);
        }
    }
    Ok(())
}

fn validate_milestones(
    env: &Env,
    milestones: &Vec<MilestoneData>,
    goal_amount: i128,
) -> Result<(), Error> {
    for i in 1..milestones.len() {
        let prev = &milestones.get(i - 1).unwrap();
        let current = &milestones.get(i).unwrap();

        if prev.target_amount >= current.target_amount {
            panic_with_error(env, Error::InvalidMilestones);
        }
    }

    if let Some(last_milestone) = milestones.last() {
        if last_milestone.target_amount != goal_amount {
            panic_with_error(env, Error::MilestoneMismatch);
        }
    } else {
        panic_with_error(env, Error::InvalidMilestones);
    }

    Ok(())
}

/// Panics the contract execution with the given error code.
/// With `contracterror`, `Error` implements `Into<soroban_sdk::Error>` directly.
fn panic_with_error(env: &Env, error: Error) -> ! {
    env.panic_with_error(error)
}

/// Validates campaign status transitions; panics if invalid.
///
/// Valid transitions:
///   Active -> GoalReached (goal reached)
///   Active -> Ended (deadline passes)
///   GoalReached -> Ended (deadline passes)
///   Active/GoalReached/Ended -> Cancelled (by creator)
pub fn validate_campaign_transition(
    env: &Env,
    current_status: &CampaignStatus,
    next_status: &CampaignStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        (CampaignStatus::Active, CampaignStatus::GoalReached) => Ok(()),
        (CampaignStatus::Active, CampaignStatus::Ended) => Ok(()),
        (CampaignStatus::Active, CampaignStatus::Cancelled) => Ok(()),
        (CampaignStatus::GoalReached, CampaignStatus::Ended) => Ok(()),
        (CampaignStatus::GoalReached, CampaignStatus::Cancelled) => Ok(()),
        (CampaignStatus::Ended, CampaignStatus::Cancelled) => Ok(()),
        (CampaignStatus::Cancelled, _) => {
            panic_with_error(env, Error::InvalidCampaignTransition);
        }
        _ => {
            panic_with_error(env, Error::InvalidCampaignTransition);
        }
    }
}

/// Validates milestone status transitions; panics if invalid.
///
/// Valid transitions:
///   Locked -> Unlocked (target_amount reached)
///   Unlocked -> Released (explicitly released)
///   Locked -> Released (direct release)
pub fn validate_milestone_transition(
    env: &Env,
    current_status: &MilestoneStatus,
    next_status: &MilestoneStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        (MilestoneStatus::Locked, MilestoneStatus::Unlocked) => Ok(()),
        (MilestoneStatus::Locked, MilestoneStatus::Released) => Ok(()),
        (MilestoneStatus::Unlocked, MilestoneStatus::Released) => Ok(()),
        (MilestoneStatus::Released, _) => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
        (MilestoneStatus::Unlocked, MilestoneStatus::Locked) => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
        _ => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
    }
}
