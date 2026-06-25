//! OrbitChain campaign smart contract.
//!
//! This is the canonical campaign implementation for the repository: it owns
//! the production campaign lifecycle, milestone handling, refunds,
//! freeze/upgrade controls, analytics views, and all new campaign features.
//!
//! `crates/contracts/core/` remains a legacy reference contract and should not
//! be used for new campaign development.

#![no_std]
// `Events::publish` and a few call sites on `Ledger` are marked deprecated in
// soroban-sdk 26.x in favour of `#[contractevent]` and the new ledger APIs.
// Migrating every call site here is tracked as a follow-up issue; suppressing
// the warning keeps CI clean without changing the published event topics.
#![allow(deprecated)]

pub mod contract;
pub mod event;
pub mod get_all_milestones;
pub mod get_milestone;
pub mod multi_asset_release;
pub mod release_milestone;
pub mod storage;
pub mod types;
pub mod views;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};
use storage::{
    acquire_lock, get_campaign, get_donor, get_donor_asset_donation, get_milestone,
    increment_donor_asset_donation, is_frozen, release_lock, set_campaign, set_donor, set_frozen,
    set_milestone, storage_get_donation_count, storage_get_release_count, storage_get_total_raised,
    storage_get_unique_donor_count, storage_increment_asset_raised,
    storage_increment_donation_count, storage_increment_unique_donor_count,
    storage_set_total_raised,
};

use types::{
    AssetInfo, CampaignData, CampaignInitializedEvent, CampaignReport, CampaignStatus,
    CampaignStatusResponse, DashboardMetrics, DonorRecord, Error, MilestoneData, MilestoneStatus,
    PlatformSummary, StellarAsset,
};

pub const VERSION: u32 = 1;

/// Refund window duration: 30 days in seconds.
/// Refunds are only permitted within this window after campaign end or cancellation.
pub const REFUND_WINDOW: u64 = 30 * 24 * 60 * 60;

/// Maximum amount of ledger time a campaign deadline may be extended.
///
/// Capping extensions at ten years keeps deadline arithmetic meaningful for
/// views, refund windows, milestone release metadata, and downstream reports.
pub const MAX_DEADLINE_GAP_SECONDS: u64 = 10 * 365 * 24 * 60 * 60;

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

        let milestone_count = milestones.len();
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
            created_at_ledger: env.ledger().sequence(),
            created_at_time: env.ledger().timestamp(),
            concluded_at_ledger: None,
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
                asset_count: accepted_assets.len(),
                milestone_count,
                created_at_ledger: env.ledger().sequence(),
            },
        );

        Ok(())
    }

    /// Issue #194 – Donate to the campaign, enforcing campaign status.
    ///
    /// Issue #242 – Reentrancy protection: acquires lock at entry, releases at exit.
    /// Issue #243 – Authorization: `donor.require_auth()`.
    ///
    /// Panics with `Error::CampaignNotActive` unless status is `Active` or `GoalReached`.
    ///
    /// Issue #195 – After updating raised_amount, loops over milestones and unlocks
    ///              any whose target_amount <= raised_amount and status == Locked.
    /// Issue #198 – After donation, transitions to GoalReached if raised_amount >= goal_amount.
    pub fn donate(env: Env, donor: Address, amount: i128, asset: AssetInfo) {
        // Issue #242 – Reentrancy protection: acquire lock
        acquire_lock(&env);

        // Issue #243 – Authorization check
        donor.require_auth();

        // Freeze check — reject all mutating operations while frozen
        if is_frozen(&env) {
            panic_with_error(&env, Error::ContractFrozen);
        }

        let mut campaign: CampaignData =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        // Issue #194 – status check: only Active or GoalReached campaigns accept donations
        match campaign.status {
            CampaignStatus::Active | CampaignStatus::GoalReached => {}
            _ => panic_with_error(&env, Error::CampaignNotActive),
        }

        if amount <= 0
            || (campaign.min_donation_amount > 0 && amount < campaign.min_donation_amount)
        {
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

        // Update TotalRaised storage
        let new_total = storage_get_total_raised(&env)
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error(&env, Error::Overflow));
        storage_set_total_raised(&env, new_total);

        // Track per-asset donation for pro-rata refund calculation
        let asset_address = get_token_address_for_asset(&env, &asset, &campaign);
        storage_increment_asset_raised(&env, &asset_address, amount);
        increment_donor_asset_donation(&env, &donor, &asset_address, amount);

        let _donor_record =
            get_donor(&env, &donor).unwrap_or(DonorRecord::new_for(donor.clone(), asset.clone()));
        // Update donor record
        let existing_donor = get_donor(&env, &donor);
        let is_new_donor = existing_donor.is_none();
        let mut donor_record =
            existing_donor.unwrap_or_else(|| DonorRecord::new_for(donor.clone(), asset.clone()));

        donor_record.apply_donation(
            &env,
            amount,
            env.ledger().timestamp(),
            env.ledger().sequence(),
            asset.clone(),
        );
        set_donor(&env, &donor, &donor_record);
        storage_increment_donation_count(&env);
        if is_new_donor {
            storage_increment_unique_donor_count(&env);
        }

        // Issue #195 – milestone unlock check
        for i in 0..campaign.milestone_count {
            if let Some(mut milestone) = get_milestone(&env, i) {
                if milestone.status == MilestoneStatus::Locked
                    && campaign.raised_amount >= milestone.target_amount
                {
                    milestone.status = MilestoneStatus::Unlocked;
                    set_milestone(&env, i, &milestone);
                    // Emit milestone_unlocked event
                    event::milestone_unlocked(
                        &env,
                        i,
                        milestone.target_amount,
                        campaign.raised_amount,
                    );
                }
            }
        }

        // Emit donation_received event
        let asset_code = resolve_asset_code(&env, &asset, &campaign);
        event::donation_received(
            &env,
            &donor,
            amount,
            asset_code,
            campaign.raised_amount,
            env.ledger().timestamp(),
        );

        // Issue #242 – Release reentrancy lock
        release_lock(&env);
    }

    /// Issue #197 – Returns the total amount raised by the campaign.
    /// No auth required. Returns 0 if no donations yet.
    pub fn get_total_raised(env: Env) -> i128 {
        storage_get_total_raised(&env)
    }

    /// Returns the number of accepted donation calls.
    pub fn get_donation_count(env: Env) -> u64 {
        storage_get_donation_count(&env)
    }

    /// Returns the number of unique donors tracked by this campaign.
    pub fn get_donor_count(env: Env) -> u32 {
        storage_get_unique_donor_count(&env)
    }

    /// Returns the number of completed milestone releases.
    pub fn get_release_count(env: Env) -> u64 {
        storage_get_release_count(&env)
    }

    /// Returns all tracked campaign transactions: donations plus releases.
    pub fn get_total_tx_count(env: Env) -> u64 {
        storage_get_donation_count(&env)
            .checked_add(storage_get_release_count(&env))
            .unwrap_or_else(|| panic_with_error(&env, Error::Overflow))
    }

    /// Returns dashboard-ready campaign analytics.
    pub fn get_campaign_report(env: Env) -> Option<CampaignReport> {
        get_campaign(&env).map(|campaign| build_campaign_report(&env, campaign))
    }

    /// Returns export-friendly aggregate counters for this contract instance.
    pub fn get_platform_summary(env: Env) -> PlatformSummary {
        let total_campaigns = if get_campaign(&env).is_some() { 1 } else { 0 };
        let active_campaigns = active_campaign_count(&env);
        let total_donations = storage_get_donation_count(&env);
        let total_releases = storage_get_release_count(&env);
        let total_transactions = total_donations
            .checked_add(total_releases)
            .unwrap_or_else(|| panic_with_error(&env, Error::Overflow));

        PlatformSummary {
            total_campaigns,
            active_campaigns,
            total_donations,
            total_releases,
            total_transactions,
        }
    }

    /// Returns compact metrics for campaign dashboards.
    pub fn get_dashboard_metrics(env: Env) -> DashboardMetrics {
        let summary = Self::get_platform_summary(env);
        DashboardMetrics {
            total_campaigns: summary.total_campaigns,
            active_campaigns: summary.active_campaigns,
            total_donations: summary.total_donations,
            total_releases: summary.total_releases,
            total_transactions: summary.total_transactions,
        }
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
    /// 2. Refunds are allowed per campaign status
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

        let refund_eligibility = check_refund_eligibility(&env, &campaign, &donor_record);
        refund_eligibility.is_ok()
    }

    /// Claim a refund for a donation.
    ///
    /// Issue #242 – Reentrancy protection: acquires lock at entry, releases at exit.
    /// Issue #243 – Authorization: `donor.require_auth()`.
    /// Issue #244 – Balance verification: checks contract balance before each transfer.
    ///
    /// # Panics
    /// - `Error::NotInitialized` if campaign not initialized
    /// - `Error::NoDonorRecord` if donor has never donated
    /// - `Error::RefundNotPermitted` if milestone already released
    /// - `Error::RefundWindowClosed` if current time > end_time + REFUND_WINDOW
    /// - `Error::RefundAlreadyClaimed` if donor already claimed refund
    /// - `Error::InsufficientContractBalance` if contract lacks funds for a transfer
    pub fn claim_refund(env: Env, donor: Address) {
        // Issue #242 – Reentrancy protection: acquire lock
        acquire_lock(&env);

        // Issue #243 – Authorization check
        donor.require_auth();

        // Freeze check — reject all mutating operations while frozen
        if is_frozen(&env) {
            panic_with_error(&env, Error::ContractFrozen);
        }

        let campaign =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        let _donor_record =
            get_donor(&env, &donor).unwrap_or_else(|| panic_with_error(&env, Error::NoDonorRecord));

        let mut donor_record =
            get_donor(&env, &donor).unwrap_or_else(|| panic_with_error(&env, Error::NoDonorRecord));

        let refund_eligibility = check_refund_eligibility(&env, &campaign, &donor_record);
        match refund_eligibility {
            Ok(_) => {
                // Calculate total released across all milestones
                let mut total_released: i128 = 0;
                for i in 0..campaign.milestone_count {
                    if let Some(milestone) = get_milestone(&env, i) {
                        total_released += milestone.released_amount;
                    }
                }

                // Calculate refund multiplier: (raised - released) / raised
                let refund_numerator = campaign.raised_amount - total_released;
                let refund_denominator = campaign.raised_amount;

                // Mark refund as claimed early to prevent reentrancy
                donor_record.refund_claimed = true;
                set_donor(&env, &donor, &donor_record);

                // For each asset the donor contributed to, calculate and transfer refund
                for asset in campaign.accepted_assets.iter() {
                    let asset_address = match &asset.issuer {
                        Some(addr) => addr.clone(),
                        None => continue, // Skip assets without an issuer (native XLM handled separately)
                    };

                    // Get amount donor contributed in this asset
                    let donor_asset_amount = get_donor_asset_donation(&env, &donor, &asset_address);

                    if donor_asset_amount > 0 {
                        // Calculate pro-rata refund: (donor_amount * refund_numerator) / refund_denominator
                        // PR #21: anti-dust floor via calculate_refund_amount helper.
                        let refund_amount = calculate_refund_amount(
                            &env,
                            donor_asset_amount,
                            refund_numerator,
                            refund_denominator,
                        );

                        if refund_amount > 0 {
                            // Issue #244 – Verify contract balance before transfer
                            use soroban_sdk::token;
                            let token_client = token::Client::new(&env, &asset_address);
                            let contract_balance =
                                token_client.balance(&env.current_contract_address());
                            if contract_balance < refund_amount {
                                panic_with_error(&env, Error::InsufficientContractBalance);
                            }

                            // Transfer refund to donor
                            token_client.transfer(
                                &env.current_contract_address(),
                                &donor,
                                &refund_amount,
                            );

                            // Emit event for this asset's refund
                            env.events().publish(
                                ("campaign", "asset_refund"),
                                (donor.clone(), asset_address, refund_amount),
                            );
                        }
                    }
                }

                // Emit overall refund claimed event
                env.events().publish(
                    ("campaign", "refund_claimed"),
                    (&donor, donor_record.total_donated),
                );

                // Issue #242 – Release reentrancy lock
                release_lock(&env);
            }
            Err(err) => panic_with_error(&env, err),
        }
    }

    /// Issue #212 – End the campaign early.
    ///
    /// Issue #243 – Authorization: `creator.require_auth()`.
    /// Transitions to `Ended` status. No refunds after milestones are released.
    pub fn end_campaign(env: Env) {
        contract::end_campaign(&env);
    }

    /// Issue #214 – Cancel the campaign.
    ///
    /// Issue #243 – Authorization: `creator.require_auth()`.
    /// Transitions to `Cancelled` status. All donors become refund-eligible.
    pub fn cancel_campaign(env: Env) {
        contract::cancel_campaign(&env);
    }

    /// Issue #215 – Extend the campaign deadline.
    ///
    /// Issue #243 – Authorization: `creator.require_auth()`.
    /// Only callable while campaign is Active or GoalReached.
    /// New deadline must be in the future and no more than ten years from the
    /// current ledger timestamp.
    pub fn extend_deadline(env: Env, new_end_time: u64) {
        contract::extend_deadline(&env, new_end_time);
    }

    /// Issue #235 – Get campaign status with computed fields.
    /// No auth required (read-only view).
    pub fn get_campaign_status(env: Env) -> CampaignStatusResponse {
        contract::get_campaign_status(&env)
    }

    /// Issue #45 – Get campaign hours remaining until deadline.
    /// No auth required (read-only view).
    pub fn get_campaign_hours_remaining(env: Env) -> u32 {
        contract::get_campaign_hours_remaining(&env)
    }

    /// Issue #207 – Release a single milestone (all assets proportionally).
    ///
    /// Issue #242 – Reentrancy protection: acquires lock at entry, releases at exit.
    /// Issue #243 – Authorization: `creator.require_auth()`.
    /// Issue #244 – Balance verification: checks contract balance before each transfer.
    pub fn release_milestone(env: Env, milestone_index: u32, recipient: Address) {
        // Issue #243 – Authorization: hoisted here so mock_all_auths() in tests
        // can intercept require_auth() within the contract invocation frame.
        let campaign =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));
        campaign.creator.require_auth();
        release_milestone::release_milestone(&env, milestone_index, recipient);
    }

    /// Issue #208 – Multi-asset milestone release with proportional distribution.
    ///
    /// Issue #242 – Reentrancy protection: acquires lock at entry, releases at exit.
    /// Issue #243 – Authorization: `creator.require_auth()`.
    /// Issue #244 – Balance verification: checks contract balance before each transfer.
    pub fn release_milestone_multi_asset(env: Env, milestone_index: u32, recipient: Address) {
        // Issue #243 – Authorization: hoisted here so mock_all_auths() in tests
        // can intercept require_auth() within the contract invocation frame.
        let campaign =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));
        campaign.creator.require_auth();
        multi_asset_release::release_milestone_multi_asset(&env, milestone_index, recipient);
    }

    /// Issue #199 – Get milestone view (raw data).
    /// No auth required (read-only view).
    pub fn get_milestone_view(env: Env, index: u32) -> MilestoneData {
        get_milestone::get_milestone_view(&env, index)
    }

    /// Issue #200 – Get all milestones (enriched views).
    /// No auth required (read-only view).
    pub fn get_all_milestones(env: Env) -> Vec<views::MilestoneView> {
        get_all_milestones::get_all_milestones_view(&env)
    }

    /// Issue #246 – Upgrade the contract's WASM hash.
    ///
    /// Only the admin (creator address stored at initialization) can call this.
    /// Emits `contract_upgraded` event on success.
    ///
    /// # Panics
    /// - `Error::Unauthorized` if not called by the creator
    /// - `Error::NotInitialized` if campaign not yet initialized
    /// - `Error::ContractFrozen` if the contract is currently frozen
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let campaign =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        campaign.creator.require_auth();

        // Freeze check — consistent with donate(), claim_refund(), and release_milestone()
        if is_frozen(&env) {
            panic_with_error(&env, Error::ContractFrozen);
        }

        // Actually deploy the new WASM hash to the contract
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        let timestamp = env.ledger().timestamp();
        event::contract_upgraded(&env, &campaign.creator, new_wasm_hash, timestamp);
    }

    /// Issue #246 – Freeze the contract, blocking all mutating operations.
    ///
    /// Only the admin (creator) can call this.
    /// While frozen, all write operations are rejected with `Error::ContractFrozen`.
    ///
    /// # Panics
    /// - `Error::Unauthorized` if not called by the creator
    /// - `Error::NotInitialized` if campaign not yet initialized
    pub fn freeze(env: Env) {
        let campaign =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        campaign.creator.require_auth();

        set_frozen(&env, true);

        let timestamp = env.ledger().timestamp();
        event::contract_frozen(&env, &campaign.creator, timestamp);
    }

    /// Issue #246 – Unfreeze the contract, re-enabling mutating operations.
    ///
    /// Only the admin (creator) can call this.
    ///
    /// # Panics
    /// - `Error::Unauthorized` if not called by the creator
    /// - `Error::NotInitialized` if campaign not yet initialized
    pub fn unfreeze(env: Env) {
        let campaign =
            get_campaign(&env).unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        campaign.creator.require_auth();

        set_frozen(&env, false);

        let timestamp = env.ledger().timestamp();
        event::contract_unfrozen(&env, &campaign.creator, timestamp);
    }
}

/// Issue #175 – assert the current invoker is the campaign creator.
///
/// Reads the creator address from campaign storage and calls `require_auth()`.
/// Panics with `Error::Unauthorized` if the campaign is not initialized;
/// Soroban's auth framework panics if the invoker is not the creator.
#[allow(dead_code)]
fn require_creator(env: &Env) {
    let campaign = get_campaign(env).unwrap_or_else(|| panic_with_error(env, Error::Unauthorized));
    campaign.creator.require_auth();
}

/// Validates that `asset` is in the campaign's accepted list and returns the
/// token contract address needed to construct a `token::Client`.
fn get_token_address_for_asset(env: &Env, asset: &AssetInfo, campaign: &CampaignData) -> Address {
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
        if asset.asset_code.is_empty() {
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

/// Resolves the asset code string for an AssetInfo.
/// For Native XLM returns "XLM"; for Stellar(addr) looks up the code in accepted_assets.
fn resolve_asset_code(env: &Env, asset: &AssetInfo, campaign: &CampaignData) -> String {
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

/// Panics the contract execution with the given error code.
fn panic_with_error(env: &Env, error: Error) -> ! {
    env.panic_with_error(error)
}

fn check_refund_eligibility(
    env: &Env,
    campaign: &CampaignData,
    donor_record: &DonorRecord,
) -> Result<(), Error> {
    // Check 1: Campaign must be in terminal state
    if !campaign.status.is_terminal() {
        return Err(Error::RefundNotPermitted);
    }

    // Check 2: Refunds allowed based on campaign status
    match campaign.status {
        CampaignStatus::Cancelled => {
            // Refunds always allowed for cancelled campaigns
        }
        CampaignStatus::Ended => {
            // Refunds only if NO milestones have been released
            for i in 0..campaign.milestone_count {
                if let Some(milestone) = get_milestone(env, i) {
                    if milestone.status == MilestoneStatus::Released {
                        return Err(Error::RefundNotPermitted);
                    }
                }
            }
        }
        _ => return Err(Error::RefundNotPermitted),
    }

    // Check 3: Current time within refund window (≤ end_time + REFUND_WINDOW)
    let current_time = env.ledger().timestamp();
    if current_time > campaign.end_time + REFUND_WINDOW {
        return Err(Error::RefundWindowClosed);
    }

    // Check 4: Donor must not have already claimed refund
    if donor_record.refund_claimed {
        return Err(Error::RefundAlreadyClaimed);
    }

    Ok(())
}

/// Validates campaign status transitions; panics if invalid.
///
/// Returns `Result<(), Error>` which is already `#[must_use]`, so no extra
/// attribute is needed (clippy `double_must_use`).
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
/// Returns `Result<(), Error>` which is already `#[must_use]`, so no extra
/// attribute is needed (clippy `double_must_use`).
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

#[cfg(test)]
mod test {
    pub mod claim_refund_tests;
    pub mod get_campaign_status_tests;
    pub mod integration_tests;
    pub mod invariant_tests;
    pub mod negative_path_tests;
    pub mod refund_eligibility_tests;
    pub mod release_milestone_tests;

    /// Shared helper: register the contract and run the body inside
    /// `env.as_contract()` so storage, ledger, and auth work correctly.
    /// Call `env.mock_all_auths()` BEFORE this if auth is needed.
    pub(crate) fn with_contract<F, R>(env: &soroban_sdk::Env, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let contract_id = env.register_contract(None, crate::CampaignContract);
        env.as_contract(&contract_id, f)
    }
}

pub(crate) fn calculate_refund_amount(
    env: &Env,
    donor_asset_amount: i128,
    refund_numerator: i128,
    refund_denominator: i128,
) -> i128 {
    if refund_denominator <= 0 {
        panic_with_error(env, Error::Overflow);
    }

    let numerator = donor_asset_amount
        .checked_mul(refund_numerator)
        .unwrap_or_else(|| panic_with_error(env, Error::Overflow));

    let refund = numerator / refund_denominator;

    // PR #21: anti-dust floor — if the donor is entitled to something nonzero but
    // floor division rounded it all the way down to 0, bump to 1 unit
    // rather than letting them lose their entire refund to rounding.
    if refund == 0 && numerator > 0 {
        1
    } else {
        refund
    }
}
fn active_campaign_count(env: &Env) -> u64 {
    match get_campaign(env) {
        Some(campaign) if campaign.status.accepts_donations() => 1,
        _ => 0,
    }
}

fn build_campaign_report(env: &Env, campaign: CampaignData) -> CampaignReport {
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
            .unwrap_or_else(|| panic_with_error(env, Error::Overflow));
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
