//! Issue #146 — Soulbound donation receipts.
//!
//! Once a campaign is *finalised* — its goal is reached **and** its final
//! milestone has been released — each donor may claim a permanent,
//! non-transferable receipt attesting to what they contributed. This gives
//! donors a durable on-chain record beyond `DonorRecord`, usable for verifiable
//! donor history and weighted voting.
//!
//! ## Two deliberate deviations from the issue text
//!
//! **1. Claim (pull), not auto-mint (push).** The issue asks to "mint to donors"
//! at finalise. That is not implementable here, and would not be safe if it
//! were: donors are stored per-address under `DataKey::DonorData(donor)` with
//! no list or index, and Soroban storage has no key enumeration — so the
//! contract cannot walk its donors. Even given a list, minting to every donor
//! inside one transaction is an unbounded loop that would exceed resource
//! limits on a large campaign. Each donor therefore claims their own receipt:
//! O(1), no enumeration, and no gas cliff.
//!
//! **2. A soulbound record, not a SAC.** The issue says "soulbound SAC tokens",
//! but a Stellar Asset Contract implements the standard token interface — its
//! `transfer` cannot be made to panic, so the "transfer panics" acceptance
//! criterion is unreachable with a real SAC. The receipt is instead a record
//! keyed by donor address, with token-shaped read accessors (`balance`) and
//! `transfer` / `transfer_from` / `approve` that panic with
//! `Error::ReceiptNonTransferable`.
//!
//! Both are flagged on the issue; happy to rework if the maintainers intended
//! something else.

use soroban_sdk::{panic_with_error, Address, Env};

use crate::event;
use crate::storage::{
    get_campaign, get_donor, get_milestone, get_receipt, get_receipt_count, has_receipt,
    increment_receipt_count, set_receipt,
};
use crate::types::{DonationReceipt, Error, MilestoneStatus};

/// Whether the campaign has been finalised: goal reached **and** the final
/// milestone released.
///
/// The issue describes "after finalise", but the contract has no `finalise`
/// entrypoint — finalisation is a *condition*, so it is computed here rather
/// than stored. Both halves are required: a campaign can reach its goal while
/// milestones are still outstanding, and milestones cannot fully release
/// without the goal being met.
#[must_use]
pub fn is_finalised(env: &Env) -> bool {
    let Some(campaign) = get_campaign(env) else {
        return false;
    };

    if campaign.raised_amount < campaign.goal_amount {
        return false;
    }

    // `milestone_count` is >= 1 by the initialisation invariant, but guard
    // anyway rather than underflow on `count - 1`.
    if campaign.milestone_count == 0 {
        return false;
    }

    let last_index = campaign.milestone_count - 1;
    match get_milestone(env, last_index) {
        Some(m) => m.status == MilestoneStatus::Released,
        None => false,
    }
}

/// Claim the caller's soulbound donation receipt.
///
/// Requires the campaign to be finalised, the caller to have a donor record,
/// and the caller not to have claimed already. Panics otherwise — a receipt is
/// permanent, so every precondition is a hard failure rather than a no-op.
pub fn claim_receipt(env: &Env, donor: Address) -> DonationReceipt {
    donor.require_auth();

    if !is_finalised(env) {
        panic_with_error!(env, Error::CampaignNotFinalised);
    }

    // Reuse the existing NoDonorRecord code: "you never donated" is the same
    // condition the refund path already reports.
    let record =
        get_donor(env, &donor).unwrap_or_else(|| panic_with_error!(env, Error::NoDonorRecord));

    if has_receipt(env, &donor) {
        panic_with_error!(env, Error::ReceiptAlreadyClaimed);
    }

    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    let receipt = DonationReceipt {
        donor: donor.clone(),
        amount_donated: record.total_donated,
        campaign_goal: campaign.goal_amount,
        minted_at: env.ledger().timestamp(),
        minted_at_ledger: env.ledger().sequence(),
    };

    set_receipt(env, &donor, &receipt);
    increment_receipt_count(env);

    event::receipt_minted(
        env,
        &donor,
        receipt.amount_donated,
        get_receipt_count(env),
        receipt.minted_at,
    );

    receipt
}

/// Read a donor's receipt. `None` if unclaimed.
#[must_use]
pub fn get_receipt_for(env: &Env, donor: Address) -> Option<DonationReceipt> {
    get_receipt(env, &donor)
}

/// Whether a donor holds a receipt.
#[must_use]
pub fn has_claimed(env: &Env, donor: Address) -> bool {
    has_receipt(env, &donor)
}

/// Token-shaped balance: `1` if the address holds a receipt, else `0`.
///
/// Receipts are one-per-donor, so this is a boolean in token clothing — it
/// exists so wallets and indexers that speak the token interface can read
/// receipt ownership.
#[must_use]
pub fn balance(env: &Env, donor: Address) -> i128 {
    i128::from(has_receipt(env, &donor))
}

/// Total receipts claimed so far.
#[must_use]
pub fn total_supply(env: &Env) -> u32 {
    get_receipt_count(env)
}

/// Soulbound enforcement: receipts can never move.
///
/// Present, and panicking, on purpose. A missing entrypoint would merely make
/// transfers impossible; an explicit panic makes the *intent* legible on-chain
/// and gives callers a precise error instead of "no such function".
pub fn transfer(env: &Env, _from: Address, _to: Address, _amount: i128) {
    panic_with_error!(env, Error::ReceiptNonTransferable);
}

/// Soulbound enforcement: delegated transfers are impossible too.
pub fn transfer_from(env: &Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
    panic_with_error!(env, Error::ReceiptNonTransferable);
}

/// Soulbound enforcement: no allowances, since nothing can be moved.
pub fn approve(env: &Env, _from: Address, _spender: Address, _amount: i128, _expiry: u32) {
    panic_with_error!(env, Error::ReceiptNonTransferable);
}
