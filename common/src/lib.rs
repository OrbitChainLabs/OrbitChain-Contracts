//! Common types shared across the OrbitChain workspace.
//!
//! This crate provides canonical definitions for `CampaignStatus`, `MilestoneStatus`,
//! `AssetInfo`, and — since issue #103 — `CommonError`: a single-source-of-truth
//! set of error variants that are semantically generic (not campaign-specific)
//! and can be reused across crates without duplicating discriminants.
//!
//! ## Error design (issue #103)
//!
//! `CommonError` carries only *non-contract-specific* variants — concepts that
//! exist identically in every contract (overflow, uninitialised state, generic
//! authorisation failure).  It is intentionally **not** a `#[contracterror]`
//! enum: that attribute assigns stable `u32` discriminants that become part of
//! the on-chain ABI, and two `#[contracterror]` enums with the same numeric
//! value produce identical on-chain error codes, making them indistinguishable
//! to callers.
//!
//! Contract-specific crates (e.g. `orbitchain-campaign`) own a single
//! `#[contracterror]` enum whose variants *import their semantics* from the
//! descriptions here.  The numeric codes used in campaign are stable and listed
//! in `campaign/src/types.rs`; this crate's `CommonError` acts as the
//! documentation and semantic authority, not as a second discriminant space.
//!
//! The deprecated reference core contract keeps its separate `CoreError` until
//! it is retired (see `crates/contracts/core/src/lib.rs`).

#![no_std]
// Re-link `std` only under `cargo test` so the `#[cfg(test)]` modules in
// `version::tests` (and any future sibling) can use `Vec`, `format!`,
// `assert!`, etc. Per no_std crate convention the `extern crate std;`
// must live at the crate root to re-establish the std crate's
// presence in the test target, since `#![no_std]` excludes it from the
// normal prelude.
#[cfg(test)]
extern crate std;
use soroban_sdk::contracttype;

// ── CommonError ───────────────────────────────────────────────────────────────

/// Workspace-wide semantic error catalogue.
///
/// These variants describe *generic* failure modes that appear in multiple
/// contracts.  They are intentionally **not** a `#[contracterror]` type so
/// they cannot collide with any contract's stable on-chain error codes.
///
/// Each contract that uses one of these concepts should define a matching
/// variant in its own `#[contracterror]` enum and document the mapping back
/// to the canonical name here.  This keeps the on-chain discriminant space
/// contract-local while providing a single authoritative description for each
/// error concept.
///
/// ### Mapping to campaign error codes (campaign/src/types.rs)
///
/// | `CommonError` variant        | `campaign::Error` variant        | code |
/// |------------------------------|----------------------------------|------|
/// | `Overflow`                   | `Overflow`                       | 17   |
/// | `NotInitialized`             | `NotInitialized`                 | 2    |
/// | `Unauthorized`               | `Unauthorized`                   | 3    |
/// | `InvalidAmount`              | `InvalidAmount`                  | 70   |
/// | `StorageWriteError`          | `StorageWriteError`              | 26   |
///
/// ### Mapping to legacy core error codes (crates/contracts/core/src/lib.rs)
///
/// | `CommonError` variant        | `CoreError` variant              | code |
/// |------------------------------|----------------------------------|------|
/// | `Overflow`                   | *(not present — use campaign)*   | —    |
/// | `NotInitialized`             | `NotInitialized`                 | 7    |
/// | `Unauthorized`               | `Unauthorized`                   | 5    |
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CommonError {
    /// A checked arithmetic operation overflowed.
    ///
    /// Contracts must use `checked_add` / `checked_sub` and surface this error
    /// rather than silently saturating, because saturation would corrupt
    /// financial accounting (total raised, refund calculations, etc.).
    ///
    /// Mapped to `campaign::Error::Overflow` (code 17).
    Overflow,

    /// The contract has not been initialised yet.
    ///
    /// Raised when a call-site reads required state (campaign record,
    /// admin address, etc.) and finds it absent.
    ///
    /// Mapped to `campaign::Error::NotInitialized` (code 2).
    NotInitialized,

    /// The caller is not authorised to perform the operation.
    ///
    /// Surfaced when `require_auth()` succeeds for the transaction signer but
    /// the signer does not hold the required role (e.g. creator-only functions
    /// called by a donor).
    ///
    /// Mapped to `campaign::Error::Unauthorized` (code 3).
    Unauthorized,

    /// A generic negative or otherwise invalid amount was supplied.
    ///
    /// Raised when a numeric argument is outside the accepted range (e.g.
    /// negative donation amount, zero release amount).
    ///
    /// Mapped to `campaign::Error::InvalidAmount` (code 70).
    InvalidAmount,

    /// A storage write operation failed.
    ///
    /// Raised when the Soroban host rejects a persistent write (entry too
    /// large, quota exceeded, etc.).
    ///
    /// Mapped to `campaign::Error::StorageWriteError` (code 26).
    StorageWriteError,
}

#[cfg(test)]
mod common_error_tests {
    use super::CommonError;

    /// Verify that CommonError variants are distinct (no enum duplicates).
    #[test]
    fn common_error_variants_are_distinct() {
        // Each variant must compare not-equal to every other variant.
        let variants = [
            CommonError::Overflow,
            CommonError::NotInitialized,
            CommonError::Unauthorized,
            CommonError::InvalidAmount,
            CommonError::StorageWriteError,
        ];
        for (i, a) in variants.iter().enumerate() {
            for b in &variants[i + 1..] {
                assert_ne!(a, b, "CommonError variants must all be distinct");
            }
        }
    }

    /// Spot-check that Copy + Clone are derived correctly.
    #[test]
    fn common_error_is_copy_and_clone() {
        let e = CommonError::Overflow;
        let cloned = e;
        assert_eq!(e, cloned);
    }
}

/// Workspace semver constants and deprecation-tracking tests.
/// See [`PROCESS.md`] and [`docs/versioning.md`] for the policy.
///
/// [`PROCESS.md`]: https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/PROCESS.md
/// [`docs/versioning.md`]: https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/docs/versioning.md
pub mod version;

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    /// Campaign is still being configured; not yet live.
    Draft,
    /// Campaign is live and accepting operations.
    Active,
    /// Campaign has successfully completed.
    Completed,
    /// Campaign was cancelled by the creator.
    Cancelled,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    /// Milestone has not yet been reached.
    Pending,
    /// Milestone has been reached and released.
    Completed,
    /// Milestone was not reached within the timeline.
    Failed,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AssetInfo {
    pub code: u32,
    pub issuer: u32,
}
