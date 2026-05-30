#![no_std]

pub mod storage;
pub mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
use types::{CampaignData, CampaignStatus, Error, MilestoneData, MilestoneStatus, StellarAsset, CampaignEvent, AssetInfo};
use storage::{get_campaign, set_campaign, set_milestone};

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
    /// - `Error::UnauthorizedCreator` if caller is not the creator or lacks authorization
    /// - `Error::AlreadyInitialized` if campaign already exists
    /// - `Error::InvalidGoalAmount` if goal_amount <= 0
    /// - `Error::InvalidEndTime` if end_time <= current ledger timestamp
    /// - `Error::InvalidAssets` if accepted_assets is empty
    /// - `Error::InvalidAssetCode` if any asset_code is empty or invalid
    /// - `Error::InvalidMilestoneCount` if milestone count is not 1-5
    /// - `Error::InvalidMilestones` if milestones are not sorted ascending by target_amount
    /// - `Error::MilestoneMismatch` if last milestone.target_amount != goal_amount
    ///
    /// # Events
    /// Emits `campaign_initialized` event with campaign details
    pub fn initialize(
        env: Env,
        creator: soroban_sdk::Address,
        goal_amount: i128,
        end_time: u64,
        accepted_assets: Vec<StellarAsset>,
        milestones: Vec<MilestoneData>,
        min_donation_amount: i128,
    ) -> Result<(), Error> {
        // Authorization check: creator must authorize this call
        creator.require_auth();

        // Check if already initialized - can only initialize once
        if get_campaign(&env).is_some() {
            panic_with_error(&env, Error::AlreadyInitialized);
        }

        // Validation 1: goal_amount > 0
        if goal_amount <= 0 {
            panic_with_error(&env, Error::InvalidGoalAmount);
        }

        // Validation 2: end_time > current ledger timestamp
        let current_timestamp = env.ledger().timestamp();
        if end_time <= current_timestamp {
            panic_with_error(&env, Error::InvalidEndTime);
        }

        // Validation 3: accepted_assets non-empty
        if accepted_assets.is_empty() {
            panic_with_error(&env, Error::InvalidAssets);
        }

        // Validation 3b: validate each asset code
        validate_assets(&env, &accepted_assets)?;

        // Validation 4: milestone count must be 1-5
        let milestone_count = milestones.len() as u32;
        if milestone_count == 0 || milestone_count > types::MAX_MILESTONES {
            panic_with_error(&env, Error::InvalidMilestoneCount);
        }

        // Validation 5 & 6: milestones sorted ascending and last == goal_amount
        validate_milestones(&env, &milestones, goal_amount)?;

        // All validations passed, store campaign data
        let campaign = CampaignData {
            creator: creator.clone(),
            goal_amount,
            raised_amount: 0,
            end_time,
            status: CampaignStatus::Active,
            accepted_assets: accepted_assets.clone(),
            milestone_count,
            min_donation_amount,
        };

        set_campaign(&env, &campaign);

        // Store each milestone
        for (index, milestone) in milestones.iter().enumerate() {
            set_milestone(&env, index as u32, milestone);
        }

        // Emit campaign_initialized event
        let event = CampaignEvent::Initialized {
            creator,
            goal_amount,
            end_time,
            asset_count: accepted_assets.len() as u32,
            milestone_count,
        };
        env.events().publish(("campaign", "initialized"), event);

        Ok(())
    }

    /// Issue #194 – Donate to the campaign, enforcing campaign status.
    ///
    /// Panics with `Error::CampaignNotActive` unless status is `Active` or `GoalReached`.
    /// The status check is atomic with the state update to prevent race conditions.
    pub fn donate(env: Env, donor: Address, amount: i128, _asset: AssetInfo) {
        donor.require_auth();

        let mut campaign: CampaignData = get_campaign(&env)
            .unwrap_or_else(|| panic_with_error(&env, Error::AlreadyInitialized));

        // Issue #194 – status check: only Active or GoalReached campaigns accept donations
        match campaign.status {
            CampaignStatus::Active | CampaignStatus::GoalReached => {}
            _ => panic_with_error(&env, Error::CampaignNotActive),
        }

        campaign.raised_amount += amount;
        set_campaign(&env, &campaign);

        env.events().publish(("campaign", "donation_received"), (donor, amount));
    }

    pub fn hello(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "campaign")
    }

    pub fn version() -> u32 {
        VERSION
    }
}

/// Helper function to validate Stellar assets
/// Ensures each asset has a non-empty asset_code
fn validate_assets(env: &Env, assets: &Vec<StellarAsset>) -> Result<(), Error> {
    for asset in assets.iter() {
        // asset_code must be non-empty
        if asset.asset_code.len() == 0 {
            panic_with_error(env, Error::InvalidAssetCode);
        }
    }
    Ok(())
}

/// Helper function to validate milestone conditions
fn validate_milestones(
    env: &Env,
    milestones: &Vec<MilestoneData>,
    goal_amount: i128,
) -> Result<(), Error> {
    // Check if milestones are sorted ascending by target_amount
    for i in 1..milestones.len() {
        let prev = &milestones.get(i - 1).unwrap();
        let current = &milestones.get(i).unwrap();

        if prev.target_amount >= current.target_amount {
            panic_with_error(env, Error::InvalidMilestones);
        }
    }

    // Check if last milestone.target_amount == goal_amount
    if let Some(last_milestone) = milestones.last() {
        if last_milestone.target_amount != goal_amount {
            panic_with_error(env, Error::MilestoneMismatch);
        }
    } else {
        panic_with_error(env, Error::InvalidMilestones);
    }

    Ok(())
}

/// Helper function to panic with a descriptive error message
fn panic_with_error(env: &Env, error: Error) -> ! {
    let error_name = match error {
        Error::InvalidGoalAmount => "InvalidGoalAmount",
        Error::InvalidEndTime => "InvalidEndTime",
        Error::InvalidAssets => "InvalidAssets",
        Error::InvalidAssetCode => "InvalidAssetCode",
        Error::InvalidMilestones => "InvalidMilestones",
        Error::MilestoneMismatch => "MilestoneMismatch",
        Error::InvalidMilestoneCount => "InvalidMilestoneCount",
        Error::AlreadyInitialized => "AlreadyInitialized",
        Error::UnauthorizedCreator => "UnauthorizedCreator",
        Error::InvalidCampaignTransition => "InvalidCampaignTransition",
        Error::InvalidMilestoneTransition => "InvalidMilestoneTransition",
        Error::CampaignNotActive => "CampaignNotActive",
        Error::CampaignEnded => "CampaignEnded",
        Error::GoalNotReached => "GoalNotReached",
        Error::DonationTooSmall => "DonationTooSmall",
    };
    env.panic_with_error(soroban_sdk::Symbol::new(env, error_name))
}

/// Validates campaign status transitions and panics if invalid
/// 
/// Valid transitions:
///   Active -> GoalReached (when goal reached)
///   Active -> Ended (when deadline passes)
///   GoalReached -> Ended (when deadline passes)
///   Active/GoalReached/Ended -> Cancelled (by creator)
pub fn validate_campaign_transition(
    env: &Env,
    current_status: &CampaignStatus,
    next_status: &CampaignStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        // Active can transition to GoalReached, Ended, or Cancelled
        (CampaignStatus::Active, CampaignStatus::GoalReached) => Ok(()),
        (CampaignStatus::Active, CampaignStatus::Ended) => Ok(()),
        (CampaignStatus::Active, CampaignStatus::Cancelled) => Ok(()),
        
        // GoalReached can transition to Ended or Cancelled
        (CampaignStatus::GoalReached, CampaignStatus::Ended) => Ok(()),
        (CampaignStatus::GoalReached, CampaignStatus::Cancelled) => Ok(()),
        
        // Ended can only transition to Cancelled
        (CampaignStatus::Ended, CampaignStatus::Cancelled) => Ok(()),
        
        // Cancelled is terminal
        (CampaignStatus::Cancelled, _) => {
            panic_with_error(env, Error::InvalidCampaignTransition);
        }
        
        // All other transitions are invalid
        _ => {
            panic_with_error(env, Error::InvalidCampaignTransition);
        }
    }
}

/// Validates milestone status transitions and panics if invalid
/// 
/// Valid transitions:
///   Locked -> Unlocked (when target_amount reached)
///   Unlocked -> Released (when explicitly released)
///   Locked -> Released (direct transition allowed)
pub fn validate_milestone_transition(
    env: &Env,
    current_status: &MilestoneStatus,
    next_status: &MilestoneStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        // Locked can transition to Unlocked or Released
        (MilestoneStatus::Locked, MilestoneStatus::Unlocked) => Ok(()),
        (MilestoneStatus::Locked, MilestoneStatus::Released) => Ok(()),
        
        // Unlocked can transition to Released
        (MilestoneStatus::Unlocked, MilestoneStatus::Released) => Ok(()),
        
        // Released is terminal
        (MilestoneStatus::Released, _) => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
        
        // Prevent Unlocked -> Locked (going backwards)
        (MilestoneStatus::Unlocked, MilestoneStatus::Locked) => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
        
        // All other transitions are invalid
        _ => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
    }
}

