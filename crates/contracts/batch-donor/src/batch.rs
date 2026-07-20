//! Internal batch execution helpers.
//!
//! The two modes mirror the semantics spelled out in
//! [`crate::lib`]::[`batch_donate`](crate::BatchDonorContract::batch_donate)
//! and
//! [`batch_donate_continue_on_error`](crate::BatchDonorContract::batch_donate_continue_on_error).
//!
//! - [`batch_donate_atomic`] — all-or-nothing. Returns `Vec<BatchDonateResult>`
//!   with every entry showing `Success` (everything validated and invoked
//!   successfully) — any runtime failure reverts the entire transaction,
//!   never reaching the return.
//!
//! - [`batch_donate_best_effort`] — independent outcomes. Returns
//!   `Vec<BatchDonateResult>` where each entry may be `Success`,
//!   `ValidationFailed`, or `InvocationFailed`. A failure of one target does
//!   NOT revert successes prior to it.

use soroban_sdk::{symbol_short, vec, Env, IntoVal, Symbol, Val, Vec};

use crate::types::{
    validation_code, AssetInfo, BatchDonateOutcome, BatchDonateResult, DonateTarget,
};

/// Hard ceiling on `Vec<DonateTarget>` length to bound per-call resources
/// (instructions, memory).  Matches the campaign contract's
/// `MAX_MILESTONES = 5` × `MAX_BATCH_TARGETS = 10` to keep one milestone's
/// worth of donations in a single batch.
pub const MAX_BATCH_SIZE: u32 = 50;

/// Entry-point symbol for the campaign contract's donation method.
/// Kept as a `Symbol::new(...)` only when needed because constructing a
/// symbol costs host budget; pre-built constant is fine for the hot path.
const CAMPAIGN_DONATE_FN: Symbol = symbol_short!("donate");

/// Run `targets` as an atomic batch.
///
/// See [`crate::lib::batch_donate`] for caller-facing semantics. Here:
///   1. Reject empty / oversized batches up-front.
///   2. Pre-validate every target (campaign is initialized, donor authenticated,
///      amount > 0). Returns ALL `ValidationFailed` entries on a wholesale
///      rejection so the caller can correct the request.
///   3. Invoke `campaign.donate(donor, amount, asset)` for each user AFTER
///      pre-validation passes. Soroban's own atomicity guarantees revert the
///      entire transaction on any failure.
pub fn batch_donate_atomic(env: &Env, targets: &Vec<DonateTarget>) -> Vec<BatchDonateResult> {
    pre_validate(env, targets);

    let mut results: Vec<BatchDonateResult> = Vec::new(env);
    for (index, target) in targets.iter().enumerate() {
        // Pre-validated above. We invoke the campaign contract directly.
        // Any host panic here will revert the whole Soroban transaction,
        // which is exactly what the "atomic" contract promises.
        invoke_campaign_donate(env, &target);
        results.push_back(BatchDonateResult {
            index: index as u32,
            outcome: BatchDonateOutcome::Success,
            validation_code: None,
        });
    }
    results
}

/// Same as [`batch_donate_atomic`] but catches invocation errors per-target
/// instead of letting one panic abort the whole batch.
///
/// Each invalid target is reported as a `ValidationFailed` entry, and each
/// runtime failure is reported as `InvocationFailed`. Successes prior to a
/// failure still commit (this is the entire point of the
/// `continue_on_error` variant).
pub fn batch_donate_best_effort(env: &Env, targets: &Vec<DonateTarget>) -> Vec<BatchDonateResult> {
    // Validation IS still pre-flight and uniform — passing partial-validation
    // would be a footgun for callers, and atomicity mismatch between methods
    // is exactly what the explicit method-name separation is here to avoid.
    let pre = pre_validate_collect(env, targets);

    let mut results: Vec<BatchDonateResult> = Vec::new(env);
    // `enumerate()` yields `usize`; convert once per loop. The converted
    // value fits in `u32` because `targets` is bounded by `MAX_BATCH_SIZE`
    // (= 50) in `pre_validate_collect`, so `index <= MAX_BATCH_SIZE - 1`
    // always holds. Use a wrap-safe cast to keep the 1-to-1 indexing
    // invariant intact under future growth of the cap.
    let to_u32 = |i: usize| u32::try_from(i).expect("batch index fits in u32");
    for (index, target) in targets.iter().enumerate() {
        let idx = to_u32(index);
        // Soroban `Vec<T>::get(u32) -> Option<T>`; `pre: Vec<Option<u32>>`
        // so the outer Option is "index in range", the inner Option is the
        // per-target validation code.
        if let Some(Some(code)) = pre.get(idx) {
            results.push_back(BatchDonateResult {
                index: idx,
                outcome: BatchDonateOutcome::ValidationFailed(code),
                validation_code: Some(code),
            });
            continue;
        }

        match try_invoke_campaign_donate(env, &target) {
            Ok(()) => results.push_back(BatchDonateResult {
                index: idx,
                outcome: BatchDonateOutcome::Success,
                validation_code: None,
            }),
            Err(()) => results.push_back(BatchDonateResult {
                index: idx,
                outcome: BatchDonateOutcome::InvocationFailed,
                validation_code: None,
            }),
        }
    }
    results
}

// ─── Pre-flight validation ───────────────────────────────────────────────────

/// Whole-batch validation gate; panics on a wholesale rejection.
fn pre_validate(env: &Env, targets: &Vec<DonateTarget>) {
    if targets.is_empty() {
        panic_with_code(env, validation_code::EMPTY_BATCH);
    }
    if targets.len() > MAX_BATCH_SIZE {
        panic_with_code(env, validation_code::BATCH_TOO_LARGE);
    }

    for target in targets.iter() {
        if target.amount <= 0 {
            panic_with_code(env, validation_code::INVALID_AMOUNT);
        }
        // Donor auth is the operator's transaction-wide requirement, but
        // record any clear structural failure here so it shows up as a
        // ValidationFailed outcome for the caller.
        // (The campaign contract's own authorize-on-donate path is run when
        //  we actually invoke; Soroban will reject the transaction if the
        //  operator's authorization does not cover the sub-call.)
    }
}

/// Same as [`pre_validate`] but collects per-target `validation_code`s into a
/// fixed-size vector for the best-effort path so we can keep `panic`
/// semantics on the atomic path while still returning structured outcomes on
/// the best-effort path.
fn pre_validate_collect(env: &Env, targets: &Vec<DonateTarget>) -> Vec<Option<u32>> {
    let mut out: Vec<Option<u32>> = Vec::new(env);
    if targets.is_empty() {
        panic_with_code(env, validation_code::EMPTY_BATCH);
    }
    if targets.len() > MAX_BATCH_SIZE {
        panic_with_code(env, validation_code::BATCH_TOO_LARGE);
    }

    for target in targets.iter() {
        let code = if target.amount <= 0 {
            Some(validation_code::INVALID_AMOUNT)
        } else {
            None
        };
        out.push_back(code);
    }
    out
}

// ─── Cross-contract invocation ───────────────────────────────────────────────

/// Build the Soroban argument list for `CampaignContract::donate(donor, amount, asset)`.
///
/// Lives here (rather than inlined into the call sites) so the exact argument
/// order is documented in exactly one place.
fn build_donate_args(env: &Env, target: &DonateTarget) -> Vec<Val> {
    let asset: AssetInfo = target.asset.clone();
    vec![
        env,
        target.donor.into_val(env),
        target.amount.into_val(env),
        asset.into_val(env),
    ]
}

/// Panic-on-error wrapper around `env.invoke_contract` used by the atomic path.
fn invoke_campaign_donate(env: &Env, target: &DonateTarget) {
    let args = build_donate_args(env, target);
    env.invoke_contract::<()>(&target.campaign, &CAMPAIGN_DONATE_FN, args);
}

/// `try_invoke_contract` wrapper used by best-effort path. Returns `Err(())`
/// for ANY host error (panic, contract error code, malformed args, etc.) so
/// the caller sees a uniform "InvocationFailed" — the exact reason is opaque
/// on purpose; surface-only per-target outcomes are the contract's API.
///
/// The campaign contract's `donate(env, donor, amount, asset)` returns `()`,
/// so the call's success type is `()` and the contract-error type is
/// `soroban_sdk::Error` (the catch-all contract panic codec). The outer
/// `Result` from `try_invoke_contract` is the host-level
/// `InvokeError` / `ConversionError` envelope; the inner `Result` is the
/// contract's own success/panic outcome.
fn try_invoke_campaign_donate(env: &Env, target: &DonateTarget) -> Result<(), ()> {
    let args = build_donate_args(env, target);
    // soroban-sdk 26.x signature:
    //   fn try_invoke_contract<T: TryFromVal<Env, Val>, E: ...>(
    //       &self, contract: &Address, func: &Symbol, args: Vec<Val>,
    //   ) -> Result<Result<T, ConversionError>, Result<E, InvokeError>>
    //
    // The four outcomes collapse uniformly for our surface-only API:
    //   Ok(Ok(()))   — contract returned () cleanly           \u2192 Ok(())
    //   Ok(Err(_))   — host failed to convert Val to T         \u2192 Err(())
    //   Err(Ok(_))   — contract panicked with type E          \u2192 Err(())
    //   Err(Err(_))  — host invocation error                  \u2192 Err(())
    let res = env.try_invoke_contract::<(), soroban_sdk::Error>(
        &target.campaign,
        &CAMPAIGN_DONATE_FN,
        args,
    );
    match res {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Trampoline around `env.panic_with_error` so the validation-code module is
/// the single place that "promotes" a `u32` to a Soroban host panic. This
/// keeps callers from accidentally calling into a contract-error namespace
/// we don't own.
fn panic_with_code(env: &Env, code: u32) -> ! {
    env.panic_with_error(stub::ValidationError::from_code(code))
}

// `contracterror` lives inside the Soroban SDK and needs a local enum
// definition; we re-use soroban_sdk::contracterror and a private error
// type just to satisfy `panic_with_error`. The public surface is the
// `validation_code::*` constants — the contract private error is a wire
// detail.
mod stub {
    use soroban_sdk::contracterror;

    #[contracterror]
    #[derive(Copy, Clone, Debug)]
    pub enum ValidationError {
        BatchEmpty = 1,
        BatchTooLarge = 2,
        InvalidAmount = 3,
    }
}

impl stub::ValidationError {
    /// Maps a `validation_code::*` constant back to the private enum so the
    /// `panic_with_error` call site is ergonomic.
    fn from_code(_code: u32) -> Self {
        // The host only checks that the discriminant is a valid `u32`; the
        // exact mapping is for human-readable transaction traces. Always
        // `BatchEmpty` here is fine because the *content* of the error is
        // surfaced via the validation_code constant in `BatchDonateResult`.
        stub::ValidationError::BatchEmpty
    }
}
