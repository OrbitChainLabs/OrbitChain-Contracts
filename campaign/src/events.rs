//! Canonical `#[contractevent]` struct definitions for the campaign contract.
//!
//! Issue #98 — Event topic canonicalisation: each struct here auto-generates a
//! hashed topic via the `#[contractevent]` macro, replacing the deprecated
//! `Events::publish` string-topic tuples used in `event.rs`.
//!
//! During the migration window the backward-compatibility shim in `event.rs`
//! emits **both** the old string-based topics and these new hashed topics so
//! off-chain consumers have time to switch.  After the deprecation window the
//! shim will be removed and call sites will use these structs directly.

use soroban_sdk::{contractevent, Address, BytesN, String};

// ── Domain events ─────────────────────────────────────────────────────────────

/// Emitted after every successful donation, once storage has been updated.
#[contractevent]
pub struct DonationReceived {
    pub donor: Address,
    pub amount: i128,
    pub asset_code: String,
    pub raised_total: i128,
    pub timestamp: u64,
}

/// Emitted once per milestone when its target is first reached.
/// Not re-emitted if the milestone is already unlocked.
#[contractevent]
pub struct MilestoneUnlocked {
    pub milestone_index: u32,
    pub target_amount: i128,
    pub raised_total: i128,
}

/// Emitted after each successful token transfer during milestone release.
/// One event per asset when multi-asset release is used.
#[contractevent]
pub struct MilestoneReleased {
    pub milestone_index: u32,
    pub amount: i128,
    pub asset_code: String,
    pub recipient: Address,
    pub timestamp: u64,
}

// ── Lifecycle events ──────────────────────────────────────────────────────────

/// Emitted once when the campaign contract is successfully initialized.
#[contractevent]
pub struct CampaignInitialized {
    pub creator: Address,
    pub goal_amount: i128,
    pub end_time: u64,
    pub asset_count: u32,
    pub milestone_count: u32,
    pub created_at_ledger: u32,
}

/// Emitted when the campaign transitions to the `Ended` state.
#[contractevent]
pub struct CampaignEnded;

/// Emitted when the campaign creator cancels the campaign.
#[contractevent]
pub struct CampaignCancelled {
    pub creator: Address,
}

/// Emitted when the campaign's raised amount reaches or exceeds the goal.
#[contractevent]
pub struct CampaignGoalReached {
    pub raised_amount: i128,
}

/// Emitted when the campaign creator extends the campaign deadline.
#[contractevent]
pub struct DeadlineExtended {
    pub creator: Address,
    pub old_deadline: u64,
    pub new_deadline: u64,
}

/// Emitted when a donor successfully claims a refund.
#[contractevent]
pub struct RefundClaimed {
    pub donor: Address,
    pub total_donated: i128,
}

/// Emitted once per asset when a donor's pro-rata refund is transferred.
#[contractevent]
pub struct AssetRefund {
    pub donor: Address,
    pub asset_address: Address,
    pub refund_amount: i128,
}

// ── Admin events ──────────────────────────────────────────────────────────────

/// Emitted when the contract WASM hash is upgraded by the admin.
#[contractevent]
pub struct ContractUpgraded {
    pub admin: Address,
    pub new_wasm_hash: BytesN<32>,
    pub timestamp: u64,
}

/// Emitted when the contract is frozen by the admin.
#[contractevent]
pub struct ContractFrozen {
    pub admin: Address,
    pub timestamp: u64,
}

/// Emitted when the contract is unfrozen by the admin.
#[contractevent]
pub struct ContractUnfrozen {
    pub admin: Address,
    pub timestamp: u64,
}

/// Emitted when an asset is blocked by the admin.
#[contractevent]
pub struct AssetBlocked {
    pub admin: Address,
    pub asset: Address,
    pub timestamp: u64,
}

/// Emitted when an asset is unblocked by the admin.
#[contractevent]
pub struct AssetUnblocked {
    pub admin: Address,
    pub asset: Address,
    pub timestamp: u64,
}

// ── Internal events ───────────────────────────────────────────────────────────

/// Emitted during multi-asset release when an asset without an issuer is
/// encountered and gracefully skipped.
#[contractevent]
pub struct MilestoneSkipped {
    pub milestone_index: u32,
    pub asset_code: String,
}
