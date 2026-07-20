//! Cross-contract types for the batch-donor contract.
//!
//! Kept intentionally minimal — the batch-donor contract does NOT take a
//! hard dependency on any other campaign contract crate (this avoids a
//! circular dependency if campaign later wants to call back into batch-donor
//! from a future feature, and lets batch-donor fan out to ANY contract that
//! exposes a compatible `donate(env, donor, amount, asset)` signature).
//!
//! The `AssetInfo` enum mirrors `orbitchain_campaign::types::AssetInfo` so a
//! batch call into the canonical campaign contract works without any extra
//! conversion glue on the wire — both enums share the same
//! `#[contracttype]` discriminant layout.

use soroban_sdk::{contracttype, Address};

/// Mirrors `orbitchain_campaign::types::AssetInfo`.
///
/// Soroban's `#[contracttype]` derives a portable XDR discriminant; both
/// sides agree on the layout:
///   0 → `Native`
///   1 → `Stellar(Address)`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetInfo {
    /// Native XLM. The campaign contract resolves the wrapped XLM token
    /// address from its own `accepted_assets` list.
    Native,
    /// A SEP-41 token contract address.
    Stellar(Address),
}

impl AssetInfo {
    /// `true` when this represents native XLM.
    #[must_use]
    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native)
    }
}

/// A single donation target inside a batch.
///
/// The full type matches the `donate(env, donor, amount, asset)` entrypoint of
/// `orbitchain_campaign::CampaignContract`. If that entrypoint ever gains
/// additional positional arguments, `DonateTarget` should grow in lock-step
/// — which is the whole point of having a dedicated struct rather than
/// passing loose tuples.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonateTarget {
    /// The address of the campaign contract instance being donated to.
    pub campaign: Address,
    /// The Stellar address that should be recorded as the donor on the
    /// target campaign's books.
    pub donor: Address,
    /// Donation amount in target asset base units.
    pub amount: i128,
    /// Which asset the amount is denominated in.
    pub asset: AssetInfo,
}

/// Per-target outcome of a batched call.
///
/// The contract returns a `Vec<BatchDonateResult>` with one entry per
/// `DonateTarget`, in the same order as the input.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchDonateOutcome {
    /// Pre-flight validation passed and the call was attempted.
    /// For atomic mode this is always terminal; for best-effort mode it
    /// means the invocation succeeded.
    Success,
    /// Pre-flight validation rejected the target before any cross-contract
    /// invocation was attempted. The `u32` is one of [`BatchDonateValidationCode`].
    ValidationFailed(u32),
    /// Pre-flight validation passed but the cross-contract invocation failed
    /// at runtime. Only possible in best-effort mode (atomic mode reverts
    /// the entire transaction on the first such failure).
    InvocationFailed,
}

/// Numeric discriminants for [`BatchDonateOutcome::ValidationFailed`].
///
/// Mirrored as `u32` rather than a `#[contracterror]` enum so the same
/// value can ride in a Soroban `Val` over the wire without colliding with
/// the campaign contract's error space (which intentionally lives only
/// inside `orbitchain_campaign::types::Error`).
pub mod validation_code {
    /// Target's `campaign` argument did not point at a deployed, initialized,
    /// and accepting-donations campaign contract.
    pub const CAMPAIGN_NOT_READY: u32 = 1;
    /// Donor's address failed authorization (target's `donor.require_auth()`
    /// would have panicked). Reported up-front in pre-flight to keep atomic
    /// mode in the all-or-nothing regime.
    pub const DONOR_UNAUTHORIZED: u32 = 2;
    /// `amount` was non-positive or otherwise rejected by the target's
    /// minimum-donation rules.
    pub const INVALID_AMOUNT: u32 = 3;
    /// Internal guard against an empty `Vec<DonateTarget>` to keep the batch
    /// genuinely "multi".
    pub const EMPTY_BATCH: u32 = 4;
    /// `Vec<DonateTarget>` exceeded `MAX_BATCH_SIZE`.
    pub const BATCH_TOO_LARGE: u32 = 5;
}

/// Per-target result returned by a batch invocation.
///
/// The result vector is indexed 1-to-1 against the input `Vec<DonateTarget>`.
/// Lengths always match unless `outcome == ValidationFailed` for the whole
/// batch (in which case the same validation code is reported for every
/// target via `BatchDonateResult::index_in_batch`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchDonateResult {
    /// Index in the original `Vec<DonateTarget>`.
    pub index: u32,
    /// The outcome.
    pub outcome: BatchDonateOutcome,
    /// When `outcome == ValidationFailed(u32)`, the numeric
    /// `validation_code::*` constant. `None` for other outcomes.
    pub validation_code: Option<u32>,
}
