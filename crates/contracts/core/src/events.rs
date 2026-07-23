//! Typed events emitted by the legacy OrbitChain core contract.

use soroban_sdk::{contractevent, Address, Env, String, Symbol};

#[contractevent]
pub struct CampaignCreated {
    #[topic]
    pub creator: Address,
    pub campaign_id: u64,
}

#[contractevent]
pub struct DonationReceived {
    #[topic]
    pub donor: Address,
    #[topic]
    pub campaign_id: u64,
    pub amount: i128,
    pub asset: Symbol,
    pub memo: String,
}

#[contractevent]
pub struct WithdrawalRequested {
    #[topic]
    pub creator: Address,
    #[topic]
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
}

#[contractevent]
pub struct WithdrawalApproved {
    #[topic]
    pub admin: Address,
    #[topic]
    pub campaign_id: u64,
    pub amount: i128,
}

#[contractevent]
pub struct TransactionSubmitted {
    #[topic]
    pub admin: Address,
    #[topic]
    pub campaign_id: u64,
    pub amount: i128,
}

pub fn campaign_created(env: &Env, creator: Address, campaign_id: u64) {
    CampaignCreated { creator, campaign_id }.publish(env);
}

pub fn donation_received(env: &Env, donor: Address, campaign_id: u64, amount: i128, asset: Symbol, memo: String) {
    DonationReceived { donor, campaign_id, amount, asset, memo }.publish(env);
}

pub fn withdrawal_requested(env: &Env, creator: Address, campaign_id: u64, recipient: Address, amount: i128) {
    WithdrawalRequested { creator, campaign_id, recipient, amount }.publish(env);
}

pub fn withdrawal_approved(env: &Env, admin: Address, campaign_id: u64, amount: i128) {
    WithdrawalApproved { admin, campaign_id, amount }.publish(env);
}

pub fn transaction_submitted(env: &Env, admin: Address, campaign_id: u64, amount: i128) {
    TransactionSubmitted { admin, campaign_id, amount }.publish(env);
}
