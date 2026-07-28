//! Typed campaign contract events.
//!
//! Each declaration uses `#[contractevent]`. The macro fixes the first topic
//! to the lower-snake-case struct name and serializes non-topic fields as a
//! named payload map.

use soroban_sdk::{contractevent, Address, BytesN, Env, String};

#[contractevent]
pub struct CampaignInitialized {
    #[topic]
    pub creator: Address,
    pub goal_amount: i128,
    pub end_time: u64,
    pub asset_count: u32,
    pub milestone_count: u32,
    pub created_at_ledger: u32,
}

#[contractevent]
pub struct DonationReceived {
    #[topic]
    pub donor: Address,
    pub amount: i128,
    pub asset_code: String,
    pub raised_total: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct MilestoneUnlocked {
    #[topic]
    pub milestone_index: u32,
    pub target_amount: i128,
    pub raised_total: i128,
}

#[contractevent]
pub struct DeadlineExtended {
    #[topic]
    pub creator: Address,
    pub old_deadline: u64,
    pub new_deadline: u64,
}

#[contractevent]
pub struct CampaignCancelled {
    #[topic]
    pub creator: Address,
}

#[contractevent]
pub struct CampaignEnded {}

#[contractevent]
pub struct MilestoneReleased {
    #[topic]
    pub milestone_index: u32,
    #[topic]
    pub recipient: Address,
    pub amount: i128,
    pub asset_code: String,
    pub timestamp: u64,
}

#[contractevent]
pub struct CampaignGoalReached {
    pub raised_total: i128,
}

#[contractevent]
pub struct AssetRefund {
    #[topic]
    pub donor: Address,
    #[topic]
    pub asset_address: Address,
    pub refund_amount: i128,
}

#[contractevent]
pub struct RefundClaimed {
    #[topic]
    pub donor: Address,
    pub total_donated: i128,
}

#[contractevent]
pub struct ContractUpgraded {
    #[topic]
    pub admin: Address,
    pub new_wasm_hash: BytesN<32>,
    pub timestamp: u64,
}

#[contractevent]
pub struct ContractFrozen {
    #[topic]
    pub admin: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct ContractUnfrozen {
    #[topic]
    pub admin: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct AssetBlocked {
    #[topic]
    pub admin: Address,
    #[topic]
    pub asset: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct AssetUnblocked {
    #[topic]
    pub admin: Address,
    #[topic]
    pub asset: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct MilestoneReleaseSkipped {
    #[topic]
    pub milestone_index: u32,
    pub asset_code: String,
    pub reason: String,
}

pub fn campaign_initialized(env: &Env, event: CampaignInitialized) { event.publish(env); }
pub fn donation_received(env: &Env, donor: &Address, amount: i128, asset_code: String, raised_total: i128, timestamp: u64) {
    DonationReceived { donor: donor.clone(), amount, asset_code, raised_total, timestamp }.publish(env);
}
pub fn milestone_unlocked(env: &Env, milestone_index: u32, target_amount: i128, raised_total: i128) {
    MilestoneUnlocked { milestone_index, target_amount, raised_total }.publish(env);
}
pub fn deadline_extended(env: &Env, creator: &Address, old_deadline: u64, new_deadline: u64) {
    DeadlineExtended { creator: creator.clone(), old_deadline, new_deadline }.publish(env);
}
pub fn campaign_cancelled(env: &Env, creator: &Address) { CampaignCancelled { creator: creator.clone() }.publish(env); }
pub fn campaign_ended(env: &Env) { CampaignEnded {}.publish(env); }
pub fn milestone_released(env: &Env, milestone_index: u32, amount: i128, asset_code: String, recipient: &Address, timestamp: u64) {
    MilestoneReleased { milestone_index, recipient: recipient.clone(), amount, asset_code, timestamp }.publish(env);
}
pub fn campaign_goal_reached(env: &Env, raised_total: i128) { CampaignGoalReached { raised_total }.publish(env); }
pub fn asset_refund(env: &Env, donor: &Address, asset_address: &Address, refund_amount: i128) {
    AssetRefund { donor: donor.clone(), asset_address: asset_address.clone(), refund_amount }.publish(env);
}
pub fn refund_claimed(env: &Env, donor: &Address, total_donated: i128) { RefundClaimed { donor: donor.clone(), total_donated }.publish(env); }
pub fn contract_upgraded(env: &Env, admin: &Address, new_wasm_hash: BytesN<32>, timestamp: u64) { ContractUpgraded { admin: admin.clone(), new_wasm_hash, timestamp }.publish(env); }
pub fn contract_frozen(env: &Env, admin: &Address, timestamp: u64) { ContractFrozen { admin: admin.clone(), timestamp }.publish(env); }
pub fn contract_unfrozen(env: &Env, admin: &Address, timestamp: u64) { ContractUnfrozen { admin: admin.clone(), timestamp }.publish(env); }
pub fn asset_blocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) { AssetBlocked { admin: admin.clone(), asset: asset.clone(), timestamp }.publish(env); }
pub fn asset_unblocked(env: &Env, admin: &Address, asset: &Address, timestamp: u64) { AssetUnblocked { admin: admin.clone(), asset: asset.clone(), timestamp }.publish(env); }
pub fn milestone_release_skipped(env: &Env, milestone_index: u32, asset_code: String, reason: String) {
    MilestoneReleaseSkipped { milestone_index, asset_code, reason }.publish(env);
}
