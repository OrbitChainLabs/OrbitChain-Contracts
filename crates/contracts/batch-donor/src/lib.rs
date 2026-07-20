//! OrbitChain batch-donor Soroban contract.
//!
//! Closes [issue #147](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/147):
//! "Add batched multi-campaign operations (single tx)".
//!
//! The contract exposes two entry points:
//!
//! | Entry point                              | Atomicity                                         | Use when…                                    |
//! |------------------------------------------|---------------------------------------------------|----------------------------------------------|
//! | [`batch_donate`]                          | **All-or-nothing** (Soroban-native)               | you want every campaign to either see all N donations or none; a failure of any one reverts the entire batch. |
//! | [`batch_donate_continue_on_error`]        | **Best-effort** — successes commit, failures reported per-target | you want to push 50 donations and tolerate a few outright failures (e.g. operator wants to know which campaigns are unreachable); each failure surfaces as `InvocationFailed` in the returned `Vec<BatchDonateResult>`. |
//!
//! Both require `operator.require_auth()` exactly once for the operator's
//! Stellar address; the target campaigns themselves re-check `donor` auth at
//! the per-campaign entry point as usual.
//!
//! ## Result shape
//!
//! Both methods return a `Vec<BatchDonateResult>` with one element per
//! `DonateTarget`, in the same order. Lengths always match. `outcome`
//! reflects either a successful invocation, a pre-flight validation
//! failure, or — for the best-effort mode only — an `InvocationFailed`
//! from the cross-contract call.
//!
//! ## Acceptance criteria reference
//!
//! Atomic semantics and partial-failure handling are documented in
//! [`PROCESS.md`](https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/PROCESS.md)
//! and again in `CHANGELOG.md`. The RustDoc on [`batch_donate`] and
//! [`batch_donate_continue_on_error`] is the canonical reference for the
//! helper test cases in [`crate::tests`].
//!
//! ## Versions
//!
//! Workspace version string is read from
//! [`common::version::VERSION_STR`] at compile time. New `#[deprecated]`
//! entry points introduced before 1.0.0 must be removed in the next
//! major bump per the policy in [`PROCESS.md`](https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/PROCESS.md)
//! § "Deprecation timeline".

#![no_std]
// Symmetric with the campaign crate: `env.panic_with_error` and a few other
// host apis are marked deprecated in soroban-sdk 26.x; suppressing the
// warning here keeps CI clean without changing emitted event topics.
#![allow(deprecated)]

mod batch;
mod types;

pub use batch::MAX_BATCH_SIZE;
pub use types::{validation_code, AssetInfo, BatchDonateOutcome, BatchDonateResult, DonateTarget};

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

/// Re-exported workspace version constants (see `common::version::VERSION_STR`).
pub use common::version;

#[contract]
pub struct BatchDonorContract;

#[contractimpl]
impl BatchDonorContract {
    /// Returns the legacy integer version of this contract, mirroring the
    /// campaign contract's `version()`. Bumped alongside `version_str()` at
    /// every contract release. Will be removed in the 1.0.0 release (see
    /// [`PROCESS.md`](https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/PROCESS.md)
    /// for the policy).
    pub fn version() -> u32 {
        common::version::VERSION_MINOR
    }

    /// Returns the workspace semver string for this contract (e.g. `"0.1.0"`).
    ///
    /// Mirrors `CampaignContract::version_str` so a single CLI call can
    /// confirm the deployed batch-donor contract matches the workspace
    /// release.
    pub fn version_str(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, version::VERSION_STR)
    }

    /// Health-check entrypoint; returns the short symbol `batch_donor`.
    ///
    /// Kept stable across releases; used by the in-repo smoke tests to
    /// confirm a freshly-loaded WASM responds.
    pub fn hello(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "batch_donor")
    }

    /// Atomic multi-campaign donation in a single Soroban transaction.
    ///
    /// # Semantics
    ///
    /// 1. `operator.require_auth()` is checked exactly once at the top of
    ///    the call and authorizes the whole sub-call fan-out.
    /// 2. Pre-flight validation rejects empty / oversized / non-positive
    ///    batches with a typed `panic_with_error` BEFORE any cross-contract
    ///    invocation is attempted.
    /// 3. Each target is then invoked via `env.invoke_contract` on the
    ///    campaign contract's `donate(donor, amount, asset)` entrypoint.
    /// 4. **Any host panic during step 3 reverts the ENTIRE transaction**
    ///    thanks to Soroban-native transaction atomicity — no partial
    ///    successes leak on-chain. The method therefore returns a
    ///    `Vec<BatchDonateResult>` whose every entry is `Success`, since
    ///    a failure could never have produced this return value.
    ///
    /// # Panics
    /// - `validation_code::EMPTY_BATCH` — `targets.is_empty()`.
    /// - `validation_code::BATCH_TOO_LARGE` — `targets.len() > MAX_BATCH_SIZE`.
    /// - `validation_code::INVALID_AMOUNT` — any `target.amount <= 0`.
    /// - Whatever the campaign contract panics with during a sub-invocation
    ///   (e.g. `CampaignNotActive`, `DonationTooSmall`, `AssetNotAccepted`).
    ///   In each case the entire batch is reverted.
    pub fn batch_donate(
        env: Env,
        operator: Address,
        targets: Vec<DonateTarget>,
    ) -> Vec<BatchDonateResult> {
        operator.require_auth();
        batch::batch_donate_atomic(&env, &targets)
    }

    /// Best-effort multi-campaign donation: collects per-target outcomes.
    ///
    /// Same `operator.require_auth()` once, same pre-flight validation as
    /// [`Self::batch_donate`]. The difference is that per-target invocation
    /// failures are caught via `env.try_invoke_contract` and reported as
    /// `BatchDonateOutcome::InvocationFailed` rather than reverting the
    /// whole transaction.
    ///
    /// Useful when the operator wants a "best effort" push to N campaigns:
    /// `Vec<BatchDonateResult>` enumerates which targets succeeded and which
    /// failed, and successfully-invoked donations remain on-chain.
    ///
    /// # Panics
    /// Same pre-flight panics as [`Self::batch_donate`]:
    /// - `validation_code::EMPTY_BATCH`,
    /// - `validation_code::BATCH_TOO_LARGE`,
    /// - `validation_code::INVALID_AMOUNT`.
    ///
    /// Runtime per-target failures DO NOT panic; they appear as
    /// `ValidationFailed` or `InvocationFailed` in the returned vector.
    pub fn batch_donate_continue_on_error(
        env: Env,
        operator: Address,
        targets: Vec<DonateTarget>,
    ) -> Vec<BatchDonateResult> {
        operator.require_auth();
        batch::batch_donate_best_effort(&env, &targets)
    }

    /// View accessor for [`MAX_BATCH_SIZE`].
    pub fn max_batch_size() -> u32 {
        MAX_BATCH_SIZE
    }
}
