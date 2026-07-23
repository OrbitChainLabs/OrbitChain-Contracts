//! Typed Soroban contract events emitted by the campaign contract.
//!
//! Each event is defined as a `#[contractevent]` struct so that:
//! 1. The contract spec includes the event type for client codegen.
//! 2. A build-time codegen tool can extract the struct definitions and
//!    emit a machine-readable JSON Schema (`codegen/schemas/events.json`).
//! 3. Call sites stay concise — the wrapper functions keep the same
//!    signatures used throughout the rest of the crate.

use soroban_sdk::{contractevent, Address, BytesN, Env, String};

// ── Namespace-prefixed events  (topics = ["campaign", "…"]) ──────────────────

/// Emitted once when the campaign contract is successfully initialized.
#[contractevent(topics = ["campaign", "initialized"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignInitialized {
    /// Campaign creator's Stellar address.
    pub creator: Address,
    /// Total funding target in base units (stroops / smallest unit).
    pub goal_amount: i128,
    /// UNIX timestamp after which donations are rejected.
    pub end_time: u64,
    /// Number of accepted assets at initialisation.
    pub asset_count: u32,
    /// Number of milestones registered.
    pub milestone_count: u32,
    /// Ledger sequence at initialisation time.
    pub created_at_ledger: u32,
}

/// Emitted when the campaign transitions to the `Ended` state (deadline
/// passed or concluded normally).
#[contractevent(topics = ["campaign", "campaign_ended"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignEnded;

/// Emitted when the campaign creator cancels the campaign.
#[contractevent(topics = ["campaign", "campaign_cancelled"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCancelled {
    /// Campaign creator's Stellar address.
    pub creator: Address,
}

/// Emitted when the campaign creator extends the campaign deadline.
#[contractevent(topics = ["campaign", "deadline_extended"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadlineExtended {
    /// Campaign creator's Stellar address.
    pub creator: Address,
    /// Previous deadline UNIX timestamp.
    pub old_deadline: u64,
    /// New deadline UNIX timestamp.
    pub new_deadline: u64,
}

/// Emitted when the campaign's raised amount reaches or exceeds the goal.
#[contractevent(topics = ["campaign", "campaign_goal_reached"], data_format = "single-value")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignGoalReached {
    /// Cumulative raised amount at the time the goal was reached.
    pub raised_amount: i128,
}

/// Emitted when a donor successfully claims a refund.
#[contractevent(topics = ["campaign", "refund_claimed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundClaimed {
    /// Donor's Stellar address.
    pub donor: Address,
    /// Total amount donated by this donor across all assets.
    pub total_donated: i128,
}

/// Emitted once per asset when a donor's pro-rata refund is transferred
/// for that asset. Multiple events may be emitted within a single
/// `claim_refund` call when the donor contributed more than one asset type.
#[contractevent(topics = ["campaign", "asset_refund"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRefund {
    /// Donor's Stellar address.
    pub donor: Address,
    /// Contract address of the refunded asset.
    pub asset_address: Address,
    /// Amount refunded in the asset's base units.
    pub refund_amount: i128,
}

/// Emitted when the contract WASM hash is upgraded by the admin.
#[contractevent(topics = ["campaign", "contract_upgraded"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUpgraded {
    /// Admin (creator) address.
    pub admin: Address,
    /// New WASM hash being deployed.
    pub new_wasm_hash: BytesN<32>,
    /// Ledger timestamp of the upgrade.
    pub timestamp: u64,
}

/// Emitted when the contract is frozen by the admin, blocking all mutating
/// operations.
#[contractevent(topics = ["campaign", "contract_frozen"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractFrozen {
    /// Admin (creator) address.
    pub admin: Address,
    /// Ledger timestamp of the freeze.
    pub timestamp: u64,
}

/// Emitted when the contract is unfrozen by the admin, re-enabling
/// mutating operations.
#[contractevent(topics = ["campaign", "contract_unfrozen"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUnfrozen {
    /// Admin (creator) address.
    pub admin: Address,
    /// Ledger timestamp of the unfreeze.
    pub timestamp: u64,
}

/// Emitted when an asset is blocked by the admin.
#[contractevent(topics = ["campaign", "asset_blocked"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetBlocked {
    /// Admin (creator) address.
    pub admin: Address,
    /// Token contract address of the blocked asset.
    pub asset: Address,
    /// Ledger timestamp of the block.
    pub timestamp: u64,
}

/// Emitted when an asset is unblocked by the admin.
#[contractevent(topics = ["campaign", "asset_unblocked"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetUnblocked {
    /// Admin (creator) address.
    pub admin: Address,
    /// Token contract address of the unblocked asset.
    pub asset: Address,
    /// Ledger timestamp of the unblock.
    pub timestamp: u64,
}

// ── Domain events  (topic derived from struct name) ──────────────────────────

/// Emitted after every successful donation, once storage has been updated.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationReceived {
    /// Donor's Stellar address.
    pub donor: Address,
    /// Donated amount in base units.
    pub amount: i128,
    /// Asset code (e.g. `"XLM"`, `"USDC"`).
    pub asset_code: String,
    /// Cumulative raised amount after this donation.
    pub raised_total: i128,
    /// Ledger timestamp of the donation.
    pub timestamp: u64,
}

/// Emitted once per milestone when its target is first reached. Not
/// re-emitted if the milestone is already unlocked.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneUnlocked {
    /// Zero-based milestone index.
    pub milestone_index: u32,
    /// Funding threshold that triggered the unlock.
    pub target_amount: i128,
    /// Cumulative raised amount at time of unlock.
    pub raised_total: i128,
}

/// Emitted after each successful token transfer during milestone release.
/// When a multi-asset release transfers tokens from multiple assets, a
/// separate event is emitted per asset.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneReleased {
    /// Zero-based milestone index.
    pub milestone_index: u32,
    /// Amount transferred in this asset's base units.
    pub amount: i128,
    /// Asset code (e.g. `"XLM"`, `"USDC"`).
    pub asset_code: String,
    /// Address that received the funds.
    pub recipient: Address,
    /// Ledger timestamp of the release.
    pub timestamp: u64,
}

// ── Wrapper functions (backwards-compatible signatures) ──────────────────────

/// Emitted when a donation is received by the campaign.
pub fn donation_received(
    env: &Env,
    donor: &Address,
    amount: i128,
    asset_code: String,
    raised_total: i128,
    timestamp: u64,
) {
    DonationReceived {
        donor: donor.clone(),
        amount,
        asset_code,
        raised_total,
        timestamp,
    }
    .publish(env);
}

/// Emitted when a milestone transitions from Locked to Unlocked.
pub fn milestone_unlocked(
    env: &Env,
    milestone_index: u32,
    target_amount: i128,
    raised_total: i128,
) {
    MilestoneUnlocked {
        milestone_index,
        target_amount,
        raised_total,
    }
    .publish(env);
}

/// Emitted when the campaign deadline is extended by the creator.
pub fn deadline_extended(env: &Env, creator: &Address, old_deadline: u64, new_deadline: u64) {
    DeadlineExtended {
        creator: creator.clone(),
        old_deadline,
        new_deadline,
    }
    .publish(env);
}

/// Emitted when the campaign is cancelled by the creator.
pub fn campaign_cancelled(env: &Env, creator: &Address) {
    CampaignCancelled {
        creator: creator.clone(),
    }
    .publish(env);
}

/// Emitted when the campaign ends (deadline passed or ended early).
pub fn campaign_ended(env: &Env) {
    CampaignEnded.publish(env);
}

/// Emitted when milestone funds are released to the recipient.
pub fn milestone_released(
    env: &Env,
    milestone_index: u32,
    amount: i128,
    asset_code: String,
    recipient: &Address,
    timestamp: u64,
) {
    MilestoneReleased {
        milestone_index,
        amount,
        asset_code,
        recipient: recipient.clone(),
        timestamp,
    }
    .publish(env);
}

/// Emitted when the contract is upgraded by the admin.
pub fn contract_upgraded(env: &Env, admin: &Address, new_wasm_hash: BytesN<32>, timestamp: u64) {
    ContractUpgraded {
        admin: admin.clone(),
        new_wasm_hash,
        timestamp,
    }
    .publish(env);
}

/// Emitted when the contract is frozen by the admin.
pub fn contract_frozen(env: &Env, admin: &Address, timestamp: u64) {
    ContractFrozen {
        admin: admin.clone(),
        timestamp,
    }
    .publish(env);
}

/// Emitted when the contract is unfrozen by the admin.
pub fn contract_unfrozen(env: &Env, admin: &Address, timestamp: u64) {
    ContractUnfrozen {
        admin: admin.clone(),
        timestamp,
    }
    .publish(env);
}

/// Emitted when an asset is blocked by the admin.
pub fn asset_blocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) {
    AssetBlocked {
        admin: admin.clone(),
        asset: asset.clone(),
        timestamp,
    }
    .publish(env);
}

/// Emitted when an asset is unblocked by the admin.
pub fn asset_unblocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) {
    AssetUnblocked {
        admin: admin.clone(),
        asset: asset.clone(),
        timestamp,
    }
    .publish(env);
}

/// Emitted when the campaign is initialized.
pub fn campaign_initialized(
    env: &Env,
    creator: &Address,
    goal_amount: i128,
    end_time: u64,
    asset_count: u32,
    milestone_count: u32,
    created_at_ledger: u32,
) {
    CampaignInitialized {
        creator: creator.clone(),
        goal_amount,
        end_time,
        asset_count,
        milestone_count,
        created_at_ledger,
    }
    .publish(env);
}

/// Emitted when the campaign goal is reached.
pub fn campaign_goal_reached(env: &Env, raised_amount: i128) {
    CampaignGoalReached { raised_amount }.publish(env);
}

/// Emitted when a donor claims a refund.
pub fn refund_claimed(env: &Env, donor: &Address, total_donated: i128) {
    RefundClaimed {
        donor: donor.clone(),
        total_donated,
    }
    .publish(env);
}

/// Emitted per-asset during a refund claim.
pub fn asset_refund(env: &Env, donor: &Address, asset_address: &Address, refund_amount: i128) {
    AssetRefund {
        donor: donor.clone(),
        asset_address: asset_address.clone(),
        refund_amount,
    }
    .publish(env);
}
