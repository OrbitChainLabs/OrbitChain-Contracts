#![no_std]
use soroban_sdk::{Address, Env, String};

/// Event emitted when a donation is received
/// 
/// # Fields
/// * `donor` - The address of the donor
/// * `amount` - The amount donated
/// * `asset` - The asset type donated
/// * `timestamp` - The timestamp of the donation
#[derive(Clone)]
pub struct DonationReceived {
    pub donor: Address,
    pub amount: i128,
    pub asset: String,
    pub timestamp: u64,
}

/// Event emitted when a withdrawal is processed
/// 
/// # Fields
/// * `recipient` - The address receiving the withdrawal
/// * `amount` - The amount withdrawn
/// * `asset` - The asset type withdrawn
/// * `timestamp` - The timestamp of the withdrawal
#[derive(Clone)]
pub struct WithdrawalProcessed {
    pub recipient: Address,
    pub amount: i128,
    pub asset: String,
    pub timestamp: u64,
}

impl DonationReceived {
    /// Emit the DonationReceived event to the ledger
    /// 
    /// # Topics (indexed for querying)
    /// - donor: Address of the donor
    /// - amount: Amount donated
    /// 
    /// # Data (full event payload)
    /// - donor: Address of the donor
    /// - amount: Amount donated  
    /// - asset: Asset type donated
    /// - timestamp: When the donation was received
    pub fn emit(&self, env: &Env) {
        env.events().publish(
            (self.donor.clone(), self.amount),
            (self.donor.clone(), self.amount, self.asset.clone(), self.timestamp),
        );
    }
}

impl WithdrawalProcessed {
    /// Emit the WithdrawalProcessed event to the ledger
    /// 
    /// # Topics (indexed for querying)
    /// - recipient: Address of the recipient
    /// - amount: Amount withdrawn
    /// 
    /// # Data (full event payload)
    /// - recipient: Address of the recipient
    /// - amount: Amount withdrawn
    /// - asset: Asset type withdrawn
    /// - timestamp: When the withdrawal was processed
    pub fn emit(&self, env: &Env) {
        env.events().publish(
            (self.recipient.clone(), self.amount),
            (self.recipient.clone(), self.amount, self.asset.clone(), self.timestamp),
        );
    }
}

/// Event type identifier for DonationReceived
/// Used by indexers to identify this event type
pub const EVENT_DONATION_RECEIVED: &[u8] = b"donation_received";

/// Event type identifier for WithdrawalProcessed  
/// Used by indexers to identify this event type
pub const EVENT_WITHDRAWAL_PROCESSED: &[u8] = b"withdrawal_processed";
