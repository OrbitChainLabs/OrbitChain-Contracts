use soroban_sdk::{contracttype, contracterror, Address, BytesN, Vec};

// ── Error enum ──────────────────────────────────────────────────────────────

/// All error types for validation and state transitions.
/// Uses `contracterror` so variants map to u32 codes and `env.panic_with_error` works.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    // ── Initialization validation errors ──
    InvalidGoalAmount        = 1,  // goal_amount must be > 0
    InvalidEndTime           = 2,  // end_time must be > current ledger timestamp
    InvalidAssets            = 3,  // accepted_assets must be non-empty
    InvalidAssetCode         = 4,  // asset_code must be non-empty and valid
    InvalidMilestones        = 5,  // milestones must be sorted ascending and last must equal goal
    MilestoneMismatch        = 6,  // last milestone.target_amount != goal_amount
    InvalidMilestoneCount    = 7,  // milestone count must be 1-5
    AlreadyInitialized       = 8,  // campaign already initialized
    UnauthorizedCreator      = 9,  // caller is not the creator or lacks authorization

    // ── State transition errors ──
    InvalidCampaignTransition  = 10, // campaign status transition not allowed
    InvalidMilestoneTransition = 11, // milestone status transition not allowed
    CampaignNotActive          = 12, // campaign must be Active to accept donations
    CampaignEnded              = 13, // campaign end_time has passed
    GoalNotReached             = 14, // cannot transition to GoalReached before reaching goal

    // ── Runtime errors ──
    NotInitialized       = 15, // campaign has not been initialized yet
    AssetNotAccepted     = 16, // donated asset is not in campaign's accepted_assets
    InvalidDonationAmount = 17, // donation amount must be > 0
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

/// Emitted by `initialize`. Stored as a `contracttype` struct so it can be
/// passed as event data via `env.events().publish(...)`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignInitializedEvent {
    pub creator: Address,
    pub goal_amount: i128,
    pub end_time: u64,
    pub asset_count: u32,
    pub milestone_count: u32,
}

/// Reusable struct for Stellar asset representation
/// Enables consistent multi-asset support across the contract
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StellarAsset {
    /// Asset code (e.g., "XLM", "USDC", "EUR")
    pub asset_code: soroban_sdk::String,
    /// Issuer address; None for native XLM (display only — transfers need a contract address)
    pub issuer: Option<Address>,
}

impl StellarAsset {
    /// Returns true when this asset is native XLM (no issuer set).
    pub fn is_xlm(&self) -> bool {
        self.issuer.is_none()
    }
}

/// Accepted asset descriptor (native XLM or a Stellar SEP-41 token).
/// Used in the `donate` function signature.
///   Native         – identifies XLM; the XLM entry in `accepted_assets` must
///                    carry the wrapped native contract address in its `issuer`.
///   Stellar(addr)  – `addr` is the token contract address directly.
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
