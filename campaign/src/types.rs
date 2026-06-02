// src/types.rs

use soroban_sdk::{contracttype, contracterror, Address, BytesN, String, Vec};

// ─── Error enum ───────────────────────────────────────────────────────────────

/// All error codes for the campaign contract.
///
/// Codes are stable — never renumber an existing variant; only append new ones.
/// Each code maps to a `u32` via `contracterror` and is surfaced in transaction
/// results as `Error(Contract, #N)`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    // ── Initialisation validation ──────────────────────────────────────── 1x
    /// `goal_amount` must be > 0.
    InvalidGoalAmount           = 1,
    /// `end_time` must be strictly greater than the current ledger timestamp.
    InvalidEndTime              = 2,
    /// `accepted_assets` must be non-empty.
    InvalidAssets               = 3,
    /// `asset_code` must be non-empty and ≤ 12 characters (Stellar limit).
    InvalidAssetCode            = 4,
    /// Milestone `target_amount` values must be strictly ascending and the
    /// last must equal `goal_amount`.
    InvalidMilestones           = 5,
    /// Last milestone `target_amount` does not equal `goal_amount`.
    MilestoneMismatch           = 6,
    /// Milestone count must be in the range [1, MAX_MILESTONES].
    InvalidMilestoneCount       = 7,
    /// `initialize` called on an already-initialised contract.
    AlreadyInitialized          = 8,
    /// Caller is not the campaign creator or lacks the required authorisation.
    UnauthorizedCreator         = 9,

    // ── State transitions ──────────────────────────────────────────────── 1x
    /// The requested campaign status transition is not permitted.
    InvalidCampaignTransition   = 10,
    /// The requested milestone status transition is not permitted.
    InvalidMilestoneTransition  = 11,
    /// Operation requires the campaign to be `Active`.
    CampaignNotActive           = 12,
    /// The campaign deadline has already passed.
    CampaignEnded               = 13,
    /// Cannot transition to `GoalReached` — raised amount < goal.
    GoalNotReached              = 14,

    // ── Runtime / call-site errors ─────────────────────────────────────── 1x
    /// Contract has not been initialised yet.
    NotInitialized              = 15,
    /// Donated asset is not in the campaign's `accepted_assets` list.
    AssetNotAccepted            = 16,
    /// Donation amount must be > 0 (and ≥ `min_donation_amount` if set).
    InvalidDonationAmount       = 17,

    // ── Arithmetic & storage ───────────────────────────────────────────── 2x
    /// A checked arithmetic operation overflowed.
    ArithmeticOverflow          = 20,
    /// A storage read returned an unexpectedly negative or invalid value.
    InvalidStorageValue         = 21,
    /// A storage write failed (entry too large, quota exceeded, etc.).
    StorageWriteError           = 22,

    // ── Asset / transfer ───────────────────────────────────────────────── 3x
    /// Recipient address is the contract itself — would lock funds permanently.
    InvalidRecipient            = 30,
    /// The asset has no issuer address; transfers require a token contract address.
    MissingIssuerAddress        = 31,
    /// Computed release amount is zero after proportional rounding.
    ZeroReleaseAmount           = 32,
    /// Release amount exceeds the contract's actual token balance.
    InsufficientContractBalance = 33,
    /// `released_amount` already equals `target_amount`; nothing left to release.
    NothingToRelease            = 34,
    /// `released_amount` would exceed `target_amount` after this operation.
    MilestoneReleasedExceedsTarget = 35,

    // ── Milestone ──────────────────────────────────────────────────────── 4x
    /// Milestone index is out of range for this campaign.
    MilestoneNotFound           = 40,
    /// Milestone is already in the `Released` state.
    MilestoneAlreadyReleased    = 41,
    /// All milestones must be Released before the campaign can be concluded.
    UnreleasedMilestonesExist   = 42,

    // ── Refunds ────────────────────────────────────────────────────────── 5x
    /// Refunds are only permitted when the campaign is `Cancelled` or
    /// `Ended` without reaching the goal.
    RefundNotPermitted          = 50,
    /// No donor record found for the requesting address.
    NoDonorRecord               = 51,
    /// Donor has already claimed a refund for this campaign.
    RefundAlreadyClaimed        = 52,
    /// Refund window has closed (> 30 days after campaign end or cancellation).
    RefundWindowClosed          = 53,

    // ── Re-entrancy / concurrency ──────────────────────────────────────── 6x
    /// A re-entrant call was detected; operation aborted.
    ReentrantCall               = 60,

    // ── Amount validation ──────────────────────────────────────────────── 7x
    /// A generic negative or otherwise invalid amount was supplied.
    InvalidAmount               = 70,
    /// Donation is below the campaign's `min_donation_amount` threshold.
    DonationBelowMinimum        = 71,
}

// ─── Campaign lifecycle ───────────────────────────────────────────────────────

/// Campaign status with documented transition rules.
///
/// ```text
/// Active ──► GoalReached ──► Ended
///   │              │           ▲
///   └──────────────┴───────────┘  (deadline passes in any non-terminal state)
///   │
///   └──► Cancelled  (creator at any point before Ended)
/// ```
///
/// Terminal states: `Ended`, `Cancelled`.
/// Only `Active` accepts new donations.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    /// Campaign is open and accepting donations.
    Active,
    /// Goal amount reached; still accepting donations until deadline.
    GoalReached,
    /// Deadline passed or campaign concluded normally.
    Ended,
    /// Creator cancelled the campaign; refunds available.
    Cancelled,
}

impl CampaignStatus {
    /// Returns `true` for states where the campaign is no longer accepting
    /// donations (`Ended` and `Cancelled`).
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Ended | Self::Cancelled)
    }

    /// Returns `true` when donations are accepted.
    pub fn accepts_donations(self) -> bool {
        matches!(self, Self::Active | Self::GoalReached)
    }

    /// Returns `true` when donors are eligible to request refunds.
    /// Refunds are available in `Cancelled` state and in `Ended` state
    /// only when the goal was not reached (enforced at the call site).
    pub fn allows_refunds(self) -> bool {
        matches!(self, Self::Cancelled | Self::Ended)
    }

    /// Validate a proposed status transition and return `Err` if it is
    /// not permitted.  Centralising the rule here prevents divergent
    /// logic across the contract.
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Active,      Self::GoalReached)
            | (Self::Active,    Self::Ended)
            | (Self::Active,    Self::Cancelled)
            | (Self::GoalReached, Self::Ended)
            | (Self::GoalReached, Self::Cancelled)
        )
    }
}

// ─── Milestone lifecycle ──────────────────────────────────────────────────────

/// Milestone status with documented transition rules.
///
/// ```text
/// Locked ──► Unlocked ──► Released
/// ```
///
/// Terminal state: `Released`.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    /// Milestone condition not yet met.
    Locked,
    /// Target amount reached; awaiting explicit release by the creator.
    Unlocked,
    /// Funds have been transferred to the beneficiary.
    Released,
}

impl MilestoneStatus {
    /// Returns `true` if the milestone is in a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Released)
    }

    /// Validate a proposed milestone status transition.
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Locked, Self::Unlocked) | (Self::Unlocked, Self::Released)
        )
    }
}

// ─── Storage keys ─────────────────────────────────────────────────────────────

/// All persistent and temporary storage keys.
///
/// Rule: never remove or renumber variants — doing so silently changes the
/// XDR discriminant and breaks existing on-chain data.  Only append new variants.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // ── Persistent ──────────────────────────────────────────────────────────
    /// Singleton campaign configuration and state.
    CampaignData,
    /// Milestone record at the given index (0-based).
    MilestoneData(u32),
    /// Aggregate donor record for the given address.
    DonorData(Address),
    /// Global total raised across all assets (i128).
    TotalRaised,
    /// Per-token raised amount — keyed by the token contract address.
    AssetRaised(Address),
    /// Per-asset donation by a donor — keyed by (donor_address, asset_address).
    /// Tracks exact amount contributed in each asset for pro-rata refund calculation.
    DonorAssetDonation(Address, Address),

    // ── Temporary ───────────────────────────────────────────────────────────
    /// Transient campaign status flag used during state transitions.
    ContractStatus,
    /// Re-entrancy guard; present = locked, absent = unlocked.
    ReentrancyLock,
}

// ─── Asset types ──────────────────────────────────────────────────────────────

/// A Stellar asset descriptor.
///
/// `issuer` is `None` only for native XLM.  All token transfers require
/// a SEP-41 contract address — callers must resolve the wrapped XLM
/// contract address themselves when `is_xlm()` returns `true`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StellarAsset {
    /// IETF-style asset code (e.g. `"XLM"`, `"USDC"`, `"EURC"`).
    /// Must be 1–12 characters matching `[A-Z0-9]`.
    pub asset_code: String,
    /// Token contract address.  `None` iff this is native XLM.
    pub issuer: Option<Address>,
}

impl StellarAsset {
    /// Returns `true` when this represents native XLM (no issuer).
    pub fn is_xlm(&self) -> bool {
        self.issuer.is_none()
    }

    /// Returns `true` when the asset code is non-empty and ≤ 12 bytes.
    /// Does not validate the character set — do that at the call site.
    pub fn has_valid_code(&self) -> bool {
        let len = self.asset_code.len();
        len > 0 && len <= 12
    }
}

/// Donation asset selector passed by the donor at call time.
///
/// `Native` — XLM; the contract resolves the wrapped XLM token address
///            from the `accepted_assets` list.
/// `Stellar(addr)` — `addr` is the SEP-41 token contract address directly.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetInfo {
    Native,
    Stellar(Address),
}

impl AssetInfo {
    /// Returns the token contract address for this asset, if known at the
    /// type level.  For `Native`, the caller must supply the wrapped address.
    pub fn token_address(&self) -> Option<&Address> {
        match self {
            Self::Native => None,
            Self::Stellar(addr) => Some(addr),
        }
    }

    /// Returns `true` for native XLM.
    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native)
    }
}

// ─── Campaign data ────────────────────────────────────────────────────────────

/// Singleton campaign configuration and runtime state.
///
/// Stored under `DataKey::CampaignData` in persistent storage.
/// Written once by `initialize`; mutated by donations, status transitions,
/// and milestone releases.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignData {
    /// Address that created the campaign and holds creator privileges.
    pub creator: Address,
    /// Total funding target in stroops / base units.
    pub goal_amount: i128,
    /// Accumulated donations across all accepted assets (base units).
    pub raised_amount: i128,
    /// UNIX timestamp (seconds) after which new donations are rejected.
    pub end_time: u64,
    /// Current lifecycle state.
    pub status: CampaignStatus,
    /// Ordered list of accepted tokens; must be non-empty.
    pub accepted_assets: Vec<StellarAsset>,
    /// Number of milestones registered at initialisation (1–MAX_MILESTONES).
    pub milestone_count: u32,
    /// Donations below this amount are rejected.  Set to `0` to disable.
    pub min_donation_amount: i128,
    /// Ledger number at which the campaign was initialised.
    pub created_at_ledger: u32,
    /// Ledger timestamp at which the campaign was initialised.
    pub created_at_time: u64,
    /// Ledger number at which the campaign entered its terminal state,
    /// if it has done so.
    pub concluded_at_ledger: Option<u32>,
}

impl CampaignData {
    /// Returns `true` when `raised_amount` has reached `goal_amount`.
    pub fn goal_reached(&self) -> bool {
        self.raised_amount >= self.goal_amount
    }

    /// Returns the remaining amount needed to reach the goal, clamped to 0.
    pub fn remaining(&self) -> i128 {
        (self.goal_amount - self.raised_amount).max(0)
    }

    /// Returns `true` when the campaign can accept a donation right now.
    /// Call-site must also check `env.ledger().timestamp() < self.end_time`.
    pub fn is_accepting_donations(&self) -> bool {
        self.status.accepts_donations()
    }
}

// ─── Milestone data ───────────────────────────────────────────────────────────

/// Maximum number of milestones per campaign.
pub const MAX_MILESTONES: u32 = 5;

/// Per-milestone funding target and release record.
///
/// Stored under `DataKey::MilestoneData(index)` in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneData {
    /// Zero-based position in the milestone sequence.
    pub index: u32,
    /// Funding threshold that must be reached to unlock this milestone.
    pub target_amount: i128,
    /// How much has been released so far.  Starts at 0; equals
    /// `target_amount - prev_milestone.target_amount` when fully released.
    pub released_amount: i128,
    /// SHA-256 hash of the off-chain milestone description document.
    pub description_hash: BytesN<32>,
    /// Current release state.
    pub status: MilestoneStatus,
    /// Ledger timestamp when the milestone was released (`Released` state only).
    pub released_at: Option<u64>,
    /// Ledger number when the milestone was released (`Released` state only).
    pub released_at_ledger: Option<u32>,
    /// Hash of the Soroban transaction that executed the release.
    pub release_tx: Option<BytesN<32>>,
    /// Address that received the milestone release funds.
    pub released_to: Option<Address>,
}

impl MilestoneData {
    /// Returns the net release amount for this milestone: the portion of
    /// `target_amount` that has not yet been released.
    pub fn pending_release(&self) -> i128 {
        (self.target_amount - self.released_amount).max(0)
    }

    /// Returns `true` when all funds for this milestone have been released.
    pub fn is_fully_released(&self) -> bool {
        self.released_amount >= self.target_amount
    }
}

// ─── Donor record ─────────────────────────────────────────────────────────────

/// Aggregate donation record for a single donor address.
///
/// Stored under `DataKey::DonorData(donor_address)` in persistent storage.
/// Updated on each subsequent donation from the same address.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonorRecord {
    /// The donor's Stellar address.
    pub donor: Address,
    /// Cumulative donated amount across all donations (base units).
    pub total_donated: i128,
    /// Asset used for the most recent donation.
    pub asset: AssetInfo,
    /// Ledger timestamp of the most recent donation.
    pub last_donation_time: u64,
    /// Ledger number of the most recent donation.
    pub last_donation_ledger: u32,
    /// Total number of individual donations made by this address.
    pub donation_count: u32,
    /// Whether this donor has already claimed a refund.
    pub refund_claimed: bool,
}

impl DonorRecord {
    /// Returns a fresh zeroed record for a first-time donor.
    pub fn new_for(donor: Address, asset: AssetInfo) -> Self {
        Self {
            donor,
            total_donated: 0,
            asset,
            last_donation_time: 0,
            last_donation_ledger: 0,
            donation_count: 0,
            refund_claimed: false,
        }
    }

    /// Apply a new donation to this record.  Returns an error string (for
    /// debug builds) rather than panicking so the call site can choose how
    /// to surface it.
    pub fn apply_donation(&mut self, amount: i128, time: u64, ledger: u32, asset: AssetInfo) {
        self.total_donated = self.total_donated.saturating_add(amount);
        self.last_donation_time = time;
        self.last_donation_ledger = ledger;
        self.donation_count = self.donation_count.saturating_add(1);
        self.asset = asset;
    }
}

// ─── Events ───────────────────────────────────────────────────────────────────

/// Emitted by `initialize`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignInitializedEvent {
    pub creator: Address,
    pub goal_amount: i128,
    pub end_time: u64,
    pub asset_count: u32,
    pub milestone_count: u32,
    pub created_at_ledger: u32,
}

/// Emitted by `donate`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationReceivedEvent {
    pub donor: Address,
    pub amount: i128,
    pub asset: AssetInfo,
    pub new_total_raised: i128,
    pub ledger: u32,
}

/// Emitted by `release_milestone_multi_asset`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneReleasedEvent {
    pub milestone_index: u32,
    pub scheduled_release: i128,
    pub total_released: i128,
    pub assets_released: u32,
    pub recipient: Address,
    pub ledger: u32,
}

/// Emitted by campaign status transitions.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignStatusChangedEvent {
    pub from: CampaignStatus,
    pub to: CampaignStatus,
    pub ledger: u32,
}

/// Emitted when a refund is processed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundProcessedEvent {
    pub donor: Address,
    pub amount: i128,
    pub asset: AssetInfo,
    pub ledger: u32,
}

use soroban_sdk::contracttype;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CampaignStatus {
    Active,
    Successful,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CampaignStatusResponse {
    pub status: CampaignStatus,
    pub days_remaining: i64,
}

pub struct Campaign {
    pub creator: Address,
    pub goal_amount: i128,
    pub raised_amount: i128,
    pub deadline: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct Campaign {
    pub creator: Address,

    pub goal_amount: i128,

    pub raised_amount: i128,

    pub deadline: u64,

    pub original_deadline: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CampaignStatus {
    Active,
    GoalReached,
    Successful,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CampaignStatus {
    Active,
    GoalReached,
    Ended,
    Successful,
    Failed,
    Cancelled,
}

