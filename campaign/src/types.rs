use soroban_sdk::{contracttype, Address, BytesN, Vec};

// ── Error enum ──────────────────────────────────────────────────────────────

/// All error types for validation and state transitions
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    // ── Initialization validation errors ──
    InvalidGoalAmount,        // goal_amount must be > 0
    InvalidEndTime,           // end_time must be > current ledger timestamp
    InvalidAssets,            // accepted_assets must be non-empty
    InvalidAssetCode,         // asset_code must be non-empty and valid
    InvalidMilestones,        // milestones must be sorted ascending and last must equal goal
    MilestoneMismatch,        // last milestone.target_amount != goal_amount
    InvalidMilestoneCount,    // milestone count must be 1-5
    AlreadyInitialized,       // campaign already initialized
    UnauthorizedCreator,      // caller is not the creator or lacks authorization
    
    // ── State transition errors ──
    InvalidCampaignTransition, // campaign status transition not allowed
    InvalidMilestoneTransition,// milestone status transition not allowed
    CampaignNotActive,        // campaign must be Active to accept donations
    CampaignEnded,            // campaign end_time has passed
    GoalNotReached,           // cannot transition to GoalReached before reaching goal
    /// Issue #192 – donation amount is below the campaign minimum
    DonationTooSmall,
}

// ── Supporting enums ─────────────────────────────────────────────────────────

/// Issue #167 – campaign lifecycle status
/// State transitions:
///   Active -> GoalReached (goal reached)
///   Active -> Ended (deadline passed)
///   GoalReached -> Ended (deadline passed)
///   Active/GoalReached/Ended -> Cancelled (by creator)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    Active,      // Campaign accepting donations
    GoalReached, // Goal amount reached, still accepting donations until deadline
    Ended,       // Deadline passed or campaign concluded
    Cancelled,   // Campaign cancelled by creator
}

/// Issue #168 – milestone release status
/// State transitions:
///   Locked -> Unlocked (when target_amount reached)
///   Unlocked -> Released (when explicitly released by admin)
///   Locked/Unlocked -> Released (milestone marked as released)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Locked,   // Milestone condition not yet met
    Unlocked, // Target amount reached, awaiting release
    Released, // Funds released to beneficiary
}

// ── Contract events ──────────────────────────────────────────────────────────

/// Campaign lifecycle events
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignEvent {
    Initialized {
        creator: Address,
        goal_amount: i128,
        end_time: u64,
        asset_count: u32,
        milestone_count: u32,
    },
}

/// Reusable struct for Stellar asset representation
/// Enables consistent multi-asset support across the contract
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StellarAsset {
    /// Asset code (e.g., "XLM", "USDC", "EUR")
    pub asset_code: soroban_sdk::String,
    /// Issuer address; None for native XLM
    pub issuer: Option<Address>,
}

impl StellarAsset {
    /// Helper function to check if this asset is native XLM
    pub fn is_xlm(&self) -> bool {
        self.issuer.is_none()
    }
}

/// Accepted asset descriptor (native XLM or a Stellar asset)
/// Deprecated: Use StellarAsset instead
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetInfo {
    Native,
    Stellar(Address),
}

// ── Issue #166 – storage key enum ────────────────────────────────────────────

/// All persistent storage keys used by the campaign contract.
/// Implements `contracttype` so Soroban can serialise it via XDR / `IntoVal`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    CampaignData,
    MilestoneData(u32),
    DonorData(Address),
    TotalRaised,
    ContractStatus,
}

// ── Issue #167 – CampaignData struct ─────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignData {
    pub creator: Address,
    pub goal_amount: i128,
    pub raised_amount: i128,
    pub end_time: u64,
    pub status: CampaignStatus,
    pub accepted_assets: Vec<StellarAsset>,
    pub milestone_count: u32,
    /// Issue #192 – minimum donation amount; set to 0 to disable enforcement
    pub min_donation_amount: i128,
}

// ── Issue #168 – MilestoneData struct ────────────────────────────────────────

/// Max 5 milestones enforced at the contract call site.
pub const MAX_MILESTONES: u32 = 5;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneData {
    pub index: u32,
    pub target_amount: i128,
    pub description_hash: BytesN<32>,
    pub status: MilestoneStatus,
    pub released_at: Option<u64>,
    pub release_tx: Option<BytesN<32>>,
}

// ── Issue #169 – DonorRecord struct ──────────────────────────────────────────

/// Stored under `DataKey::DonorData(donor_address)`.
/// Aggregated per-donor across multiple donations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonorRecord {
    pub donor: Address,
    pub total_donated: i128,
    pub asset: AssetInfo,
    pub last_donation_time: u64,
}
