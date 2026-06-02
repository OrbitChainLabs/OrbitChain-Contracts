#![no_std]

pub mod event;
pub mod storage;
pub mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};
use types::{CampaignData, CampaignInitializedEvent, CampaignStatus, DonorRecord, Error, MilestoneData, MilestoneStatus, StellarAsset, AssetInfo};
use storage::{get_campaign, set_campaign, get_milestone, set_milestone, get_donor, set_donor, get_total_raised as storage_get_total_raised, set_total_raised, increment_donor_asset_donation, get_donor_asset_donation};

pub const VERSION: u32 = 1;

/// Refund window duration: 30 days in seconds.
/// Refunds are only permitted within this window after campaign end or cancellation.
pub const REFUND_WINDOW: u64 = 30 * 24 * 60 * 60;

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
    /// - `Error::Unauthorized`   if caller is not the creator
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
        min_donation_amount: i128,
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
            min_donation_amount,
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

    /// Issue #194 – Donate to the campaign, enforcing campaign status.
    ///
    /// Panics with `Error::CampaignNotActive` unless status is `Active` or `GoalReached`.
    /// The status check is atomic with the state update to prevent race conditions.
    ///
    /// Issue #195 – After updating raised_amount, loops over milestones and unlocks
    ///              any whose target_amount <= raised_amount and status == Locked.
    /// Issue #198 – After donation, transitions to GoalReached if raised_amount >= goal_amount.
    pub fn donate(env: Env, donor: Address, amount: i128, asset: AssetInfo) {
        donor.require_auth();

        let mut campaign: CampaignData = get_campaign(&env)
            .unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        // Issue #194 – status check: only Active or GoalReached campaigns accept donations
        match campaign.status {
            CampaignStatus::Active | CampaignStatus::GoalReached => {}
            _ => panic_with_error(&env, Error::CampaignNotActive),
        }

        if amount <= 0 || (campaign.min_donation_amount > 0 && amount < campaign.min_donation_amount) {
            panic_with_error(&env, Error::DonationTooSmall);
        }

        // Issue #195 – update raised_amount atomically
        campaign.raised_amount = campaign
            .raised_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error(&env, Error::Overflow));

        // Issue #198 – goal reached status transition
        if campaign.raised_amount >= campaign.goal_amount
            && campaign.status == CampaignStatus::Active
        {
            campaign.status = CampaignStatus::GoalReached;
            env.events().publish(
                ("campaign", "campaign_goal_reached"),
                campaign.raised_amount,
            );
        }

        set_campaign(&env, &campaign);

        // Issue #195 – update TotalRaised storage
        let new_total = storage_get_total_raised(&env)
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error(&env, Error::Overflow));
        set_total_raised(&env, new_total);

        // Track per-asset donation for pro-rata refund calculation
        let asset_address = get_token_address_for_asset(&env, &asset, &campaign);
        increment_donor_asset_donation(&env, &donor, &asset_address, amount);

        // Issue #195 – update donor record
        let mut donor_record = get_donor(&env, &donor).unwrap_or(DonorRecord {
            donor: donor.clone(),
            total_donated: 0,
            asset: asset.clone(),
            last_donation_time: 0,
        });
        donor_record.total_donated = donor_record
            .total_donated
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error(&env, Error::Overflow));
        donor_record.asset = asset;
        donor_record.last_donation_time = env.ledger().timestamp();
        set_donor(&env, &donor, &donor_record);

        // Issue #195 – milestone unlock check
        for i in 0..campaign.milestone_count {
            if let Some(mut milestone) = get_milestone(&env, i) {
                if milestone.status == MilestoneStatus::Locked
                    && campaign.raised_amount >= milestone.target_amount
                {
                    milestone.status = MilestoneStatus::Unlocked;
                    set_milestone(&env, i, &milestone);
                    // Issue #229 – emit milestone_unlocked event
                    event::milestone_unlocked(&env, i, milestone.target_amount, campaign.raised_amount);
                }
            }
        }

        // Issue #228 – emit donation_received event with required schema
        let asset_code = resolve_asset_code(&env, &asset, &campaign);
        event::donation_received(&env, &donor, amount, asset_code, campaign.raised_amount, env.ledger().timestamp());
    }

    /// Issue #197 – Returns the total amount raised by the campaign.
    /// No auth required. Returns 0 if no donations yet.
    pub fn get_total_raised(env: Env) -> i128 {
        storage_get_total_raised(&env)
    }

    /// Issue #196 – Returns the donor record for the given address.
    /// No auth required. Returns None if the address has never donated.
    pub fn get_donor_record(env: Env, donor: Address) -> Option<DonorRecord> {
        get_donor(&env, &donor)
    }

    pub fn hello(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "campaign")
    }

    pub fn version() -> u32 {
        VERSION
    }

    /// Check if a donor is eligible to claim a refund.
    ///
    /// A donor is refund-eligible if ALL of the following are true:
    /// 1. Campaign is in terminal state (Ended or Cancelled)
    /// 2. Refunds are allowed per campaign status:
    ///    - `Cancelled`: always allow refunds
    ///    - `Ended`: only if NO milestones have been Released yet
    /// 3. Current time is within the refund window (≤ 30 days after end_time)
    /// 4. Donor has never claimed a refund before
    /// 5. Donor has made at least one donation
    ///
    /// This view function exposes the on-chain refund policy transparently.
    /// No auth required (read-only).
    pub fn is_refund_eligible(env: Env, donor: Address) -> bool {
        let campaign = match get_campaign(&env) {
            Some(c) => c,
            None => return false,
        };

        let donor_record = match get_donor(&env, &donor) {
            Some(d) => d,
            None => return false,
        };

        // Check 1: Campaign must be in terminal state
        if !campaign.status.is_terminal() {
            return false;
        }

        // Check 2: Refunds allowed based on campaign status
        match campaign.status {
            CampaignStatus::Cancelled => {
                // Refunds always allowed for cancelled campaigns
            }
            CampaignStatus::Ended => {
                // Refunds only if NO milestones have been released yet
                for i in 0..campaign.milestone_count {
                    if let Some(milestone) = get_milestone(&env, i) {
                        if milestone.status == MilestoneStatus::Released {
                            return false; // A milestone was already released
                        }
                    }
                }
            }
            _ => return false, // Active and GoalReached are not terminal
        }

        // Check 3: Current time within refund window (≤ end_time + REFUND_WINDOW)
        let current_time = env.ledger().timestamp();
        if current_time > campaign.end_time + REFUND_WINDOW {
            return false;
        }

        // Check 4: Donor must not have already claimed refund
        if donor_record.refund_claimed {
            return false;
        }

        true
    }

    /// Claim a refund for a donation.
    ///
    /// Eligibility Rules (enforced in order):
    /// 1. Campaign must be in a terminal state (Ended or Cancelled)
    /// 2. For Cancelled campaigns: refund always available (if within window)
    /// 3. For Ended campaigns: NO milestones must have been Released yet
    /// 4. Current time must be within the 30-day refund window after end_time
    /// 5. Donor must not have already claimed a refund
    ///
    /// Refund Calculation (Pro-rata):
    /// - Total released = sum of released_amount across all milestones
    /// - Refund % = (raised_amount - total_released) / raised_amount
    /// - Per-asset refund = donor_asset_amount × refund%
    ///
    /// Each asset is transferred separately to the donor.
    ///
    /// # Panics
    /// - `Error::NotInitialized` if campaign not initialized
    /// - `Error::NoDonorRecord` if donor has never donated
    /// - `Error::RefundNotPermitted` if milestone already released
    /// - `Error::RefundWindowClosed` if current time > end_time + REFUND_WINDOW
    /// - `Error::RefundAlreadyClaimed` if donor already claimed refund
    pub fn claim_refund(env: Env, donor: Address) {
        donor.require_auth();

        let campaign = get_campaign(&env)
            .unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        let mut donor_record = get_donor(&env, &donor)
            .unwrap_or_else(|| panic_with_error(&env, Error::NoDonorRecord));

        // Eligibility Check 1: Campaign must be terminal
        if !campaign.status.is_terminal() {
            panic_with_error(&env, Error::RefundNotPermitted);
        }

        // Eligibility Check 2-3: Status-specific rules
        match campaign.status {
            CampaignStatus::Cancelled => {
                // Refunds always allowed for cancelled campaigns
            }
            CampaignStatus::Ended => {
                // Refunds only if NO milestones have been released
                for i in 0..campaign.milestone_count {
                    if let Some(milestone) = get_milestone(&env, i) {
                        if milestone.status == MilestoneStatus::Released {
                            panic_with_error(&env, Error::RefundNotPermitted);
                        }
                    }
                }
            }
            _ => panic_with_error(&env, Error::RefundNotPermitted),
        }

        // Eligibility Check 4: Refund window
        let current_time = env.ledger().timestamp();
        if current_time > campaign.end_time + REFUND_WINDOW {
            panic_with_error(&env, Error::RefundWindowClosed);
        }

        // Eligibility Check 5: Prevent double refunds
        if donor_record.refund_claimed {
            panic_with_error(&env, Error::RefundAlreadyClaimed);
        }

        // Calculate total released across all milestones
        let mut total_released: i128 = 0;
        for i in 0..campaign.milestone_count {
            if let Some(milestone) = get_milestone(&env, i) {
                total_released += milestone.released_amount;
            }
        }

        // Calculate refund multiplier: (raised - released) / raised
        // This gives the fraction of the donation that should be refunded
        let refund_numerator = campaign.raised_amount - total_released;
        let refund_denominator = campaign.raised_amount;

        // Mark refund as claimed early to prevent reentrancy
        donor_record.refund_claimed = true;
        set_donor(&env, &donor, &donor_record);

        // For each asset the donor contributed to, calculate and transfer refund
        for asset in campaign.accepted_assets.iter() {
            let asset_address = asset.issuer.as_ref()
                .unwrap_or_else(|| panic_with_error(&env, Error::MissingIssuerAddress));
            
            // Get amount donor contributed in this asset
            let donor_asset_amount = get_donor_asset_donation(&env, &donor, asset_address);
            
            if donor_asset_amount > 0 {
                // Calculate pro-rata refund: (donor_amount * refund_numerator) / refund_denominator
                let refund_amount = (donor_asset_amount * refund_numerator) / refund_denominator;
                
                if refund_amount > 0 {
                    // Transfer refund to donor
                    use soroban_sdk::token;
                    let token_client = token::Client::new(&env, asset_address);
                    token_client.transfer(&env.current_contract_address(), &donor, &refund_amount);

                    // Emit event for this asset's refund
                    env.events().publish(
                        ("campaign", "asset_refund"),
                        (donor.clone(), asset_address.clone(), refund_amount),
                    );
                }
            }
        }

        // Emit overall refund claimed event
        env.events().publish(
            ("campaign", "refund_claimed"),
            (&donor, donor_record.total_donated),
        );
    }
}

/// Issue #175 – assert the current invoker is the campaign creator.
///
/// Reads the creator address from campaign storage and calls `require_auth()`.
/// Panics with `Error::Unauthorized` if the campaign is not initialized;
/// Soroban's auth framework panics if the invoker is not the creator.
fn require_creator(env: &Env) {
    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error(env, Error::Unauthorized));
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

#[cfg(test)]
mod test {
    pub mod refund_eligibility_tests;
}

    Ok(())
/// Resolves the asset code string for an AssetInfo.
/// For Native XLM returns "XLM"; for Stellar(addr) looks up the code in accepted_assets.
fn resolve_asset_code(env: &Env, asset: &AssetInfo, campaign: &CampaignData) -> String {
    match asset {
        AssetInfo::Native => String::from_str(env, "XLM"),
        AssetInfo::Stellar(addr) => {
            campaign
                .accepted_assets
                .iter()
                .find(|a| a.issuer == Some(addr.clone()))
                .map(|a| a.asset_code.clone())
                .unwrap_or_else(|| String::from_str(env, "UNKNOWN"))
        }
    }
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
