//! Issue #98 — Backward-compatibility shim for migrating off `Events::publish`
//! tuples to `#[contractevent]` structs.
//!
//! During the migration window, each function here emits **both** the old
//! string-based topics (for existing off-chain consumers) and the new hashed
//! topics via `crate::events::*::publish()` (for forward-looking consumers).
//!
//! After the deprecation window, call sites will use `crate::events::*`
//! directly and this module will be removed.
#![allow(deprecated)]

use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Symbol};

// ── Domain events ─────────────────────────────────────────────────────────────

/// Emitted when a donation is received by the campaign.
pub fn donation_received(
    env: &Env,
    donor: &Address,
    amount: i128,
    asset_code: String,
    raised_total: i128,
    timestamp: u64,
) {
    // New-style: hashed topic via #[contractevent]
    crate::events::DonationReceived {
        donor: donor.clone(),
        amount,
        asset_code: asset_code.clone(),
        raised_total,
        timestamp,
    }
    .publish(env);

    // Old-style: string topic (backward compat)
    let topics = (
        Symbol::new(env, "donation_received"),
        env.current_contract_address(),
    );
    env.events()
        .publish(topics, (donor, amount, asset_code, raised_total, timestamp));
}

/// Emitted when a milestone transitions from Locked to Unlocked.
pub fn milestone_unlocked(
    env: &Env,
    milestone_index: u32,
    target_amount: i128,
    raised_total: i128,
) {
    crate::events::MilestoneUnlocked {
        milestone_index,
        target_amount,
        raised_total,
    }
    .publish(env);

    let topics = (
        Symbol::new(env, "milestone_unlocked"),
        env.current_contract_address(),
    );
    env.events()
        .publish(topics, (milestone_index, target_amount, raised_total));
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
    crate::events::MilestoneReleased {
        milestone_index,
        amount,
        asset_code: asset_code.clone(),
        recipient: recipient.clone(),
        timestamp,
    }
    .publish(env);

    let topics = (
        Symbol::new(env, "milestone_released"),
        env.current_contract_address(),
    );
    env.events().publish(
        topics,
        (milestone_index, amount, asset_code, recipient, timestamp),
    );
}

// ── Lifecycle events ──────────────────────────────────────────────────────────

/// Emitted when the campaign is initialised.
pub fn campaign_initialized(
    env: &Env,
    creator: &Address,
    goal_amount: i128,
    end_time: u64,
    asset_count: u32,
    milestone_count: u32,
    created_at_ledger: u32,
) {
    crate::events::CampaignInitialized {
        creator: creator.clone(),
        goal_amount,
        end_time,
        asset_count,
        milestone_count,
        created_at_ledger,
    }
    .publish(env);

    // Old-style uses CampaignInitializedEvent from types for backward compat
    let old_event = crate::types::CampaignInitializedEvent {
        creator: creator.clone(),
        goal_amount,
        end_time,
        asset_count,
        milestone_count,
        created_at_ledger,
    };
    env.events()
        .publish(("campaign", "initialized"), old_event);
}

/// Emitted when the campaign ends (deadline passed or ended early).
pub fn campaign_ended(env: &Env) {
    crate::events::CampaignEnded {}.publish(env);
    env.events().publish(("campaign", "campaign_ended"), ());
}

/// Emitted when the campaign is cancelled by the creator.
pub fn campaign_cancelled(env: &Env, creator: &Address) {
    crate::events::CampaignCancelled {
        creator: creator.clone(),
    }
    .publish(env);
    env.events()
        .publish(("campaign", "campaign_cancelled"), creator);
}

/// Emitted when the campaign's raised amount reaches or exceeds the goal.
pub fn campaign_goal_reached(env: &Env, raised_amount: i128) {
    crate::events::CampaignGoalReached { raised_amount }.publish(env);
    env.events()
        .publish(("campaign", "campaign_goal_reached"), raised_amount);
}

/// Emitted when the campaign deadline is extended by the creator.
pub fn deadline_extended(env: &Env, creator: &Address, old_deadline: u64, new_deadline: u64) {
    crate::events::DeadlineExtended {
        creator: creator.clone(),
        old_deadline,
        new_deadline,
    }
    .publish(env);
    env.events().publish(
        ("campaign", "deadline_extended"),
        (creator, old_deadline, new_deadline),
    );
}

/// Emitted when a donor successfully claims a refund.
pub fn refund_claimed(env: &Env, donor: &Address, total_donated: i128) {
    crate::events::RefundClaimed {
        donor: donor.clone(),
        total_donated,
    }
    .publish(env);
    env.events()
        .publish(("campaign", "refund_claimed"), (donor, total_donated));
}

/// Emitted once per asset when a donor's pro-rata refund is transferred.
pub fn asset_refund(env: &Env, donor: &Address, asset_address: &Address, refund_amount: i128) {
    crate::events::AssetRefund {
        donor: donor.clone(),
        asset_address: asset_address.clone(),
        refund_amount,
    }
    .publish(env);
    env.events().publish(
        ("campaign", "asset_refund"),
        (donor, asset_address, refund_amount),
    );
}

// ── Admin events ──────────────────────────────────────────────────────────────

/// Emitted when the contract is upgraded by the admin.
pub fn contract_upgraded(
    env: &Env,
    admin: &Address,
    new_wasm_hash: BytesN<32>,
    timestamp: u64,
) {
    crate::events::ContractUpgraded {
        admin: admin.clone(),
        new_wasm_hash: new_wasm_hash.clone(),
        timestamp,
    }
    .publish(env);
    env.events().publish(
        ("campaign", "contract_upgraded"),
        (admin, new_wasm_hash, timestamp),
    );
}

/// Emitted when the contract is frozen by the admin.
pub fn contract_frozen(env: &Env, admin: &Address, timestamp: u64) {
    crate::events::ContractFrozen {
        admin: admin.clone(),
        timestamp,
    }
    .publish(env);
    env.events()
        .publish(("campaign", "contract_frozen"), (admin, timestamp));
}

/// Emitted when the contract is unfrozen by the admin.
pub fn contract_unfrozen(env: &Env, admin: &Address, timestamp: u64) {
    crate::events::ContractUnfrozen {
        admin: admin.clone(),
        timestamp,
    }
    .publish(env);
    env.events()
        .publish(("campaign", "contract_unfrozen"), (admin, timestamp));
}

/// Emitted when an asset is blocked by the admin.
pub fn asset_blocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) {
    crate::events::AssetBlocked {
        admin: admin.clone(),
        asset: asset.clone(),
        timestamp,
    }
    .publish(env);
    env.events()
        .publish(("campaign", "asset_blocked"), (admin, asset, timestamp));
}

/// Emitted when an asset is unblocked by the admin.
pub fn asset_unblocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) {
    crate::events::AssetUnblocked {
        admin: admin.clone(),
        asset: asset.clone(),
        timestamp,
    }
    .publish(env);
    env.events()
        .publish(("campaign", "asset_unblocked"), (admin, asset, timestamp));
}

// ── Internal events ───────────────────────────────────────────────────────────

/// Emitted during multi-asset release when an asset without an issuer is
/// gracefully skipped.
pub fn milestone_asset_skipped(env: &Env, milestone_index: u32, asset_code: String) {
    crate::events::MilestoneSkipped {
        milestone_index,
        asset_code: asset_code.clone(),
    }
    .publish(env);
    env.events().publish(
        (symbol_short!("ms_skip"), symbol_short!("no_issuer")),
        (milestone_index, asset_code),
    );
}
