// Issue: `Events::publish` is deprecated in soroban-sdk 26.x in favour of the
// `#[contractevent]` macro. Migrating every event definition here is a
// follow-up tracked separately; suppressing the warning keeps CI clean without
// changing the published event topics or behaviour.
#![allow(deprecated)]

use soroban_sdk::{Address, Env, String, Symbol};

/// Emitted when a donation is received by the campaign.
pub fn donation_received(
    env: &Env,
    donor: &Address,
    amount: i128,
    asset_code: String,
    raised_total: i128,
    timestamp: u64,
) {
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
    let topics = (
        Symbol::new(env, "milestone_unlocked"),
        env.current_contract_address(),
    );
    env.events()
        .publish(topics, (milestone_index, target_amount, raised_total));
}

/// Emitted when the campaign deadline is extended by the creator.
pub fn deadline_extended(env: &Env, creator: &Address, old_deadline: u64, new_deadline: u64) {
    env.events().publish(
        ("campaign", "deadline_extended"),
        (creator, old_deadline, new_deadline),
    );
}

/// Emitted when the campaign is cancelled by the creator.
pub fn campaign_cancelled(env: &Env, creator: &Address) {
    env.events()
        .publish(("campaign", "campaign_cancelled"), creator);
}

/// Emitted when the campaign ends (deadline passed or ended early).
pub fn campaign_ended(env: &Env) {
    env.events().publish(("campaign", "campaign_ended"), ());
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
    let topics = (
        Symbol::new(env, "milestone_released"),
        env.current_contract_address(),
    );
    env.events().publish(
        topics,
        (milestone_index, amount, asset_code, recipient, timestamp),
    );
}

/// Issue #246 – Emitted when the contract is upgraded by the admin.
pub fn contract_upgraded(
    env: &Env,
    admin: &Address,
    new_wasm_hash: soroban_sdk::BytesN<32>,
    timestamp: u64,
) {
    env.events().publish(
        ("campaign", "contract_upgraded"),
        (admin, new_wasm_hash, timestamp),
    );
}

/// Issue #246 – Emitted when the contract is frozen by the admin.
pub fn contract_frozen(env: &Env, admin: &Address, timestamp: u64) {
    env.events()
        .publish(("campaign", "contract_frozen"), (admin, timestamp));
}

/// Issue #246 – Emitted when the contract is unfrozen by the admin.
pub fn contract_unfrozen(env: &Env, admin: &Address, timestamp: u64) {
    env.events()
        .publish(("campaign", "contract_unfrozen"), (admin, timestamp));
}

/// Issue #90 – Emitted when an asset is blocked by the admin.
pub fn asset_blocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) {
    env.events()
        .publish(("campaign", "asset_blocked"), (admin, asset, timestamp));
}

/// Issue #90 – Emitted when an asset is unblocked by the admin.
pub fn asset_unblocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) {
    env.events()
        .publish(("campaign", "asset_unblocked"), (admin, asset, timestamp));
}
