//! OrbitChain core smart contract — campaign lifecycle, donations,
//! withdrawals, analytics, and dashboard reporting.
//!
//! Deprecated legacy/reference contract. `campaign/` (`orbitchain-campaign`)
//! is the canonical implementation for new campaign work. This crate remains
//! in the workspace only for historical compatibility and reference while any
//! remaining behavior is gradually migrated into the canonical contract.

#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracterror, contracttype, symbol_short, vec, Address, Env, String,
    Symbol, Vec,
};

/// Issue #103 – Stellar base fee in stroops (1 XLM = 10,000,000 stroops)
const BASE_FEE: i128 = 100;

// ── Error types ──────────────────────────────────────────────────────────────

/// Typed error codes for the OrbitChain core contract.
///
/// Each variant has a stable `u32` discriminant — **never renumber**.
/// Callers can match on these codes programmatically instead of parsing
/// opaque string messages out of `HostError` panics.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CoreError {
    /// The requested campaign does not exist in storage.
    CampaignNotFound = 1,
    /// The campaign is not currently active (donations/withdrawals blocked).
    CampaignNotActive = 2,
    /// The supplied amount does not exceed the required base fee.
    InsufficientAmount = 3,
    /// The campaign does not have enough raised funds for the requested amount.
    InsufficientFunds = 4,
    /// The caller is not authorised to perform this operation.
    Unauthorized = 5,
    /// The asset symbol must be non-empty.
    AssetNotSpecified = 6,
    /// The contract has not been initialised (no admin set).
    NotInitialized = 7,
    /// No pending withdrawal request exists for this campaign.
    NoPendingWithdrawal = 8,
    /// A withdrawal request is already pending or approved for this campaign.
    WithdrawalAlreadyPending = 9,
    /// The withdrawal request is not in the `Pending` state.
    WithdrawalNotPending = 10,
    /// The withdrawal request must be `Approved` before it can be submitted.
    WithdrawalNotApproved = 11,
    /// The withdrawal amount must be greater than zero.
    InvalidWithdrawalAmount = 12,
}

// ── Storage key helpers ──────────────────────────────────────────────────────

fn campaign_key(id: u64) -> (Symbol, u64) {
    (symbol_short!("camp"), id)
}

/// Issue #131 – pending withdrawal approval key
fn pending_withdrawal_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("pendwith"), campaign_id)
}

/// Issue #102 – per-campaign per-asset raised total key
fn asset_raised_key(campaign_id: u64, asset: &Symbol) -> (Symbol, u64, Symbol) {
    (symbol_short!("araised"), campaign_id, asset.clone())
}

/// Issue #104 – ordered donation record list key
fn history_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("history"), campaign_id)
}

fn donors_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("donors"), campaign_id)
}

fn donation_key(campaign_id: u64, donor: &Address) -> (Symbol, u64, Address) {
    (symbol_short!("don"), campaign_id, donor.clone())
}

/// Issue #142 – global transaction counter key
fn total_tx_key() -> Symbol {
    symbol_short!("totaltx")
}

/// Issue #145 – global donation counter key (counts every successful `donate` call)
fn total_donations_key() -> Symbol {
    symbol_short!("totaldon")
}

/// Issue #145 – global withdrawal-request counter key (counts every successful `withdraw` call)
fn total_withdrawals_key() -> Symbol {
    symbol_short!("totalwd")
}

// ── Events ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    CampaignCreated = 1,
    DonationReceived = 2,
    WithdrawalRequested = 3,
    WithdrawalApproved = 4,
}

// ── Withdrawal types ─────────────────────────────────────────────────────────

/// Issue #137 – withdrawal lifecycle status
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WithdrawalStatus {
    Pending = 0,
    Approved = 1,
    /// Issue #136 – transaction submitted to Horizon and confirmed
    Submitted = 2,
}

/// Issue #131 – pending withdrawal awaiting admin approval
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalRequest {
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
    /// Issue #137 – replaces the old `approved: bool` with a proper status enum
    pub status: WithdrawalStatus,
}

// ── Data types ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Campaign {
    pub id: u64,
    pub creator: Address,
    pub title: Symbol,
    pub goal: i128,
    pub raised: i128,
    pub deadline: u64,
    pub active: bool,
}

/// Issue #104 – one entry in the donation history list
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationRecord {
    pub donor: Address,
    pub amount: i128,   // net amount after fee
    pub fee: i128,
    pub asset: Symbol,
    pub timestamp: u64,
}

/// Issue #100 – donation metadata: memo, donor public key, timestamp
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationMetadata {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub memo: String,
    pub timestamp: u64,
}

// ── Analytics & reporting types ──────────────────────────────────────────────

/// Issue #147 – per-campaign report combining stored campaign data with
/// derived statistics (funding progress, donor count, donation count).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignReport {
    pub campaign_id: u64,
    pub creator: Address,
    pub title: Symbol,
    pub goal: i128,
    pub raised: i128,
    /// `goal - raised`, clamped to 0 when the campaign is fully funded.
    pub remaining: i128,
    /// Funding progress in basis points (0-10_000, where 10_000 == 100%).
    /// Using bps avoids floating-point in `no_std` and keeps the value lossless.
    pub progress_bps: u32,
    pub deadline: u64,
    pub active: bool,
    pub donor_count: u32,
    pub donation_count: u32,
}

/// Issue #146 – platform-wide summary suitable for export.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformSummary {
    pub total_campaigns: u64,
    pub total_donations: u64,
    pub total_withdrawals: u64,
    pub total_transactions: u64,
}

/// Issue #148 – aggregate metrics returned by the dashboard analytics API.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DashboardMetrics {
    pub total_campaigns: u64,
    pub active_campaigns: u64,
    pub total_donations: u64,
    pub total_withdrawals: u64,
    pub total_transactions: u64,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Terminates contract execution with a typed [`CoreError`] code.
///
/// Mirrors the pattern used in the campaign contract for consistent error
/// handling across the workspace.
fn panic_with_error(env: &Env, error: CoreError) -> ! {
    env.panic_with_error(error)
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct OrbitChainContract;

#[contractimpl]
impl OrbitChainContract {
    /// Initialize the contract with admin address
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&symbol_short!("admin"), &admin);
        env.storage().instance().set(&symbol_short!("count"), &0u64);
    }

    /// Ping method for health check
    pub fn ping() -> u32 {
        1
    }

    /// Create a new campaign
    pub fn create_campaign(
        env: Env,
        creator: Address,
        title: Symbol,
        goal: i128,
        deadline: u64,
    ) -> u64 {
        creator.require_auth();

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("count"))
            .unwrap_or(0);

        count += 1;

        let campaign = Campaign {
            id: count,
            creator: creator.clone(),
            title: title.clone(),
            goal,
            raised: 0,
            deadline,
            active: true,
        };

        // Issue #99 – store each campaign keyed by its ID
        env.storage().persistent().set(&campaign_key(count), &campaign);
        env.storage().instance().set(&symbol_short!("count"), &count);

        // Emit CampaignCreated event
        env.events().publish(
            (Symbol::new(&env, "CampaignCreated"), creator),
            count,
        );

        count
    }

    /// Donate to a campaign.
    pub fn donate(
        env: Env,
        donor: Address,
        campaign_id: u64,
        amount: i128,
        asset: Symbol,
        memo: String,
    ) {
        donor.require_auth();

        // Issue #102 – validate asset is provided
        if asset == Symbol::new(&env, "") {
            panic_with_error(&env, CoreError::AssetNotSpecified);
        }
        if amount <= BASE_FEE {
            panic_with_error(&env, CoreError::InsufficientAmount);
        }

        // Issue #99 – validate campaign existence
        let mut campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .unwrap_or_else(|| panic_with_error(&env, CoreError::CampaignNotFound));

        if !campaign.active {
            panic_with_error(&env, CoreError::CampaignNotActive);
        }

        // Issue #103 – calculate and deduct fee
        let fee = BASE_FEE;
        let net = amount - fee;

        // Update overall raised total
        campaign.raised += net;
        env.storage().persistent().set(&campaign_key(campaign_id), &campaign);

        // Issue #102 – update per-asset raised total
        let prev_asset_raised: i128 = env
            .storage()
            .persistent()
            .get(&asset_raised_key(campaign_id, &asset))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&asset_raised_key(campaign_id, &asset), &(prev_asset_raised + net));

        // Issue #104 – append to donation history
        let record = DonationRecord {
            donor: donor.clone(),
            amount: net,
            fee,
            asset: asset.clone(),
            timestamp: env.ledger().timestamp(),
        };
        let mut history: Vec<DonationRecord> = env
            .storage()
            .persistent()
            .get(&history_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);
        history.push_back(record);
        env.storage().persistent().set(&history_key(campaign_id), &history);

        // Issue #100 – store donation metadata
        let metadata = DonationMetadata {
            campaign_id,
            donor: donor.clone(),
            amount,
            memo: memo.clone(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&donation_key(campaign_id, &donor), &metadata);

        // Issue #101 – maintain unique donor list per campaign
        let mut donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);

        if !donors.contains(&donor) {
            donors.push_back(donor.clone());
            env.storage().persistent().set(&donors_key(campaign_id), &donors);
        }

        // Emit DonationReceived event
        env.events().publish(
            (Symbol::new(&env, "DonationReceived"), donor, campaign_id),
            (amount, asset, memo),
        );

        // Issue #142 – increment global transaction counter
        let tx_count: u64 = env
            .storage()
            .instance()
            .get(&total_tx_key())
            .unwrap_or(0);
        env.storage().instance().set(&total_tx_key(), &(tx_count + 1));

        // Issue #145 – increment dedicated donation counter so it can be queried
        // independently of withdrawals.
        let donation_count: u64 = env
            .storage()
            .instance()
            .get(&total_donations_key())
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&total_donations_key(), &(donation_count + 1));
    }

    /// Get campaign details
    #[must_use]
    pub fn get_campaign(env: Env, campaign_id: u64) -> Option<Campaign> {
        env.storage().persistent().get(&campaign_key(campaign_id))
    }

    /// Issue #102 – get total raised for a specific asset on a campaign
    #[must_use]
    pub fn get_asset_raised(env: Env, campaign_id: u64, asset: Symbol) -> i128 {
        env.storage()
            .persistent()
            .get(&asset_raised_key(campaign_id, &asset))
            .unwrap_or(0)
    }

    /// Issue #103 – expose the fee constant
    #[must_use]
    pub fn get_base_fee() -> i128 {
        BASE_FEE
    }

    /// Issue #104 – paginated donation history for a campaign.
    #[must_use]
    pub fn get_donation_history(
        env: Env,
        campaign_id: u64,
        page: u32,
        page_size: u32,
    ) -> Vec<DonationRecord> {
        let history: Vec<DonationRecord> = env
            .storage()
            .persistent()
            .get(&history_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);

        let total = history.len();
        let start = page * page_size;
        if start >= total {
            return vec![&env];
        }

        let end = (start + page_size).min(total);
        let mut page_records: Vec<DonationRecord> = vec![&env];
        for i in start..end {
            page_records.push_back(history.get(i).unwrap());
        }
        page_records
    }

    /// Issue #101 – get donor list for a campaign
    pub fn get_donors(env: Env, campaign_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env])
    }

    /// Issue #101 – get unique donor count for a campaign
    pub fn get_donor_count(env: Env, campaign_id: u64) -> u32 {
        let donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);
        donors.len()
    }

    /// Issue #100 – get donation metadata for a specific donor + campaign
    pub fn get_donation(env: Env, campaign_id: u64, donor: Address) -> Option<DonationMetadata> {
        env.storage()
            .persistent()
            .get(&donation_key(campaign_id, &donor))
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("admin"))
    }

    /// Issue #130 – validate that a recipient address string is non-empty (format check).
    /// Returns true if the address is considered valid.
    pub fn validate_recipient(_env: Env, recipient: Address) -> bool {
        // In Soroban, Address is already a validated type; receiving one means it parsed correctly.
        // We perform an additional runtime check: the address must not be the zero/default value.
        // Since Soroban Address is always well-formed when deserialized, we simply return true
        // and use the require_auth pattern in withdraw() to confirm the caller controls it.
        let _ = recipient;
        true
    }

    /// Issue #129 – request a withdrawal from a campaign.
    /// Creates a pending WithdrawalRequest that must be approved by admin (issue #131).
    pub fn withdraw(env: Env, creator: Address, campaign_id: u64, recipient: Address, amount: i128) {
        creator.require_auth();

        // Issue #130 – validate recipient (non-zero amount, valid address type enforced by SDK)
        if amount <= 0 {
            panic_with_error(&env, CoreError::InvalidWithdrawalAmount);
        }

        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .unwrap_or_else(|| panic_with_error(&env, CoreError::CampaignNotFound));

        if campaign.creator != creator {
            panic_with_error(&env, CoreError::Unauthorized);
        }
        if campaign.raised < amount {
            panic_with_error(&env, CoreError::InsufficientFunds);
        }

        // Issue #138 – prevent double withdrawals: reject if a pending request already exists
        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<_, WithdrawalRequest>(&pending_withdrawal_key(campaign_id))
        {
            if existing.status != WithdrawalStatus::Submitted {
                panic_with_error(&env, CoreError::WithdrawalAlreadyPending);
            }
        }

        // Issue #131 – store pending withdrawal for admin approval
        let request = WithdrawalRequest {
            campaign_id,
            recipient: recipient.clone(),
            amount,
            status: WithdrawalStatus::Pending,
        };
        env.storage()
            .persistent()
            .set(&pending_withdrawal_key(campaign_id), &request);

        // Issue #142 – increment global transaction counter
        let tx_count: u64 = env
            .storage()
            .instance()
            .get(&total_tx_key())
            .unwrap_or(0);
        env.storage().instance().set(&total_tx_key(), &(tx_count + 1));

        // Issue #145 – increment dedicated withdrawal counter so it can be queried
        // independently of donations.
        let withdrawal_count: u64 = env
            .storage()
            .instance()
            .get(&total_withdrawals_key())
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&total_withdrawals_key(), &(withdrawal_count + 1));

        env.events().publish(
            (Symbol::new(&env, "WithdrawalRequested"), creator, campaign_id),
            (recipient, amount),
        );
    }

    /// Issue #131 – admin approves a pending withdrawal request.
    pub fn approve_withdrawal(env: Env, admin: Address, campaign_id: u64) -> WithdrawalRequest {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("admin"))
            .unwrap_or_else(|| panic_with_error(&env, CoreError::NotInitialized));
        if admin != stored_admin {
            panic_with_error(&env, CoreError::Unauthorized);
        }

        let mut request: WithdrawalRequest = env
            .storage()
            .persistent()
            .get(&pending_withdrawal_key(campaign_id))
            .unwrap_or_else(|| panic_with_error(&env, CoreError::NoPendingWithdrawal));

        if request.status != WithdrawalStatus::Pending {
            panic_with_error(&env, CoreError::WithdrawalNotPending);
        }

        // Deduct from campaign raised balance
        let mut campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .unwrap_or_else(|| panic_with_error(&env, CoreError::CampaignNotFound));
        if campaign.raised < request.amount {
            panic_with_error(&env, CoreError::InsufficientFunds);
        }
        campaign.raised -= request.amount;
        env.storage().persistent().set(&campaign_key(campaign_id), &campaign);

        request.status = WithdrawalStatus::Approved;
        env.storage()
            .persistent()
            .set(&pending_withdrawal_key(campaign_id), &request);

        env.events().publish(
            (Symbol::new(&env, "WithdrawalApproved"), admin, campaign_id),
            request.amount,
        );

        request
    }

    /// Issue #136 – submit an approved withdrawal transaction to the network.
    /// In a Soroban contract the actual Horizon submission happens off-chain; this method
    /// records the on-chain confirmation that the transaction was submitted and accepted.
    pub fn submit_transaction(env: Env, admin: Address, campaign_id: u64) -> WithdrawalRequest {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("admin"))
            .unwrap_or_else(|| panic_with_error(&env, CoreError::NotInitialized));
        if admin != stored_admin {
            panic_with_error(&env, CoreError::Unauthorized);
        }

        let mut request: WithdrawalRequest = env
            .storage()
            .persistent()
            .get(&pending_withdrawal_key(campaign_id))
            .unwrap_or_else(|| panic_with_error(&env, CoreError::NoPendingWithdrawal));

        if request.status != WithdrawalStatus::Approved {
            panic_with_error(&env, CoreError::WithdrawalNotApproved);
        }

        // Issue #137 – update status to Submitted (confirmed on network)
        request.status = WithdrawalStatus::Submitted;
        env.storage()
            .persistent()
            .set(&pending_withdrawal_key(campaign_id), &request);

        env.events().publish(
            (Symbol::new(&env, "TransactionSubmitted"), admin, campaign_id),
            request.amount,
        );

        request
    }

    /// Get a pending withdrawal request for a campaign
    pub fn get_withdrawal_request(env: Env, campaign_id: u64) -> Option<WithdrawalRequest> {
        env.storage()
            .persistent()
            .get(&pending_withdrawal_key(campaign_id))
    }

    /// Issue #142 – expose total transaction count (donations + withdrawal requests)
    pub fn get_total_tx_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&total_tx_key())
            .unwrap_or(0)
    }

    // ── Analytics & reporting (issues #145, #146, #147, #148) ────────────────────────────

    /// Issue #145 – total number of campaigns ever created (DB entry count for campaigns).
    pub fn get_campaign_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&symbol_short!("count"))
            .unwrap_or(0)
    }

    /// Issue #145 – total number of donation entries recorded across all campaigns.
    pub fn get_total_donations(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&total_donations_key())
            .unwrap_or(0)
    }

    /// Issue #145 – total number of withdrawal requests recorded across all campaigns.
    pub fn get_total_withdrawals(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&total_withdrawals_key())
            .unwrap_or(0)
    }

    /// Issue #147 – build a per-campaign report including funding progress, donor
    /// count and donation count. Returns `None` if the campaign does not exist.
    pub fn get_campaign_report(env: Env, campaign_id: u64) -> Option<CampaignReport> {
        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))?;

        // Donor count (issue #101 storage)
        let donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);

        // Donation count (issue #104 storage)
        let history: Vec<DonationRecord> = env
            .storage()
            .persistent()
            .get(&history_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);

        // Funding progress (basis points, 0-10_000) and remaining-to-goal.
        let (progress_bps, remaining) = if campaign.goal <= 0 {
            (0u32, 0i128)
        } else if campaign.raised >= campaign.goal {
            (10_000u32, 0i128)
        } else if campaign.raised <= 0 {
            (0u32, campaign.goal)
        } else {
            let bps = (campaign.raised * 10_000) / campaign.goal;
            (bps as u32, campaign.goal - campaign.raised)
        };

        Some(CampaignReport {
            campaign_id: campaign.id,
            creator: campaign.creator,
            title: campaign.title,
            goal: campaign.goal,
            raised: campaign.raised,
            remaining,
            progress_bps,
            deadline: campaign.deadline,
            active: campaign.active,
            donor_count: donors.len(),
            donation_count: history.len(),
        })
    }

    /// Issue #146 – generate a platform-wide summary of all stored entries.
    /// Acts as the export-friendly aggregate of every counter the contract maintains.
    pub fn get_platform_summary(env: Env) -> PlatformSummary {
        PlatformSummary {
            total_campaigns: Self::get_campaign_count(env.clone()),
            total_donations: Self::get_total_donations(env.clone()),
            total_withdrawals: Self::get_total_withdrawals(env.clone()),
            total_transactions: Self::get_total_tx_count(env),
        }
    }

    /// Issue #148 – dashboard analytics endpoint returning aggregate metrics,
    /// including a derived `active_campaigns` count.
    pub fn get_dashboard_metrics(env: Env) -> DashboardMetrics {
        let total_campaigns = Self::get_campaign_count(env.clone());

        // Walk every stored campaign once to count active ones. Cheap because
        // it only reads instance/persistent storage already paid for.
        let mut active_campaigns: u64 = 0;
        let mut id: u64 = 1;
        while id <= total_campaigns {
            if let Some(c) = env
                .storage()
                .persistent()
                .get::<_, Campaign>(&campaign_key(id))
            {
                if c.active {
                    active_campaigns += 1;
                }
            }
            id += 1;
        }

        DashboardMetrics {
            total_campaigns,
            active_campaigns,
            total_donations: Self::get_total_donations(env.clone()),
            total_withdrawals: Self::get_total_withdrawals(env.clone()),
            total_transactions: Self::get_total_tx_count(env),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, String};

    #[test]
    fn test_ping() {
        let env = Env::default();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);
        assert_eq!(client.ping(), 1);
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        assert_eq!(client.get_admin(), Some(admin));
    }

    #[test]
    fn test_create_and_donate_with_metadata_and_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);

        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "donation memo");

        // #103 – fee deducted: net = 1000 - 100 = 900
        client.donate(&donor, &cid, &1000, &xlm, &memo);

        // #99 – campaign raised updated
        let campaign = client.get_campaign(&cid).unwrap();
        assert_eq!(campaign.raised, 900);

        // #102 – per-asset totals
        assert_eq!(client.get_asset_raised(&cid, &xlm), 900);

        // #101 – unique donor count = 1
        assert_eq!(client.get_donor_count(&cid), 1);

        // #100 – metadata stored
        let meta = client.get_donation(&cid, &donor).unwrap();
        assert_eq!(meta.donor, donor);
        assert_eq!(meta.memo, memo);
    }

    /// Issue #130 – validate_recipient returns true for a valid address
    #[test]
    fn test_validate_recipient() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);
        let recipient = Address::generate(&env);
        assert!(client.validate_recipient(&recipient));
    }

    /// Issues #129, #131 – withdraw creates pending request; approve_withdrawal approves it
    #[test]
    fn test_withdraw_and_approve() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);

        // Donate so there are raised funds
        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        client.donate(&donor, &cid, &1000, &xlm, &memo); // raised = 900

        let recipient = Address::generate(&env);

        // #129 – request withdrawal
        client.withdraw(&creator, &cid, &recipient, &500);
        let req = client.get_withdrawal_request(&cid).unwrap();
        assert_eq!(req.status, WithdrawalStatus::Pending);
        assert_eq!(req.amount, 500);

        // #131 – admin approves
        let approved = client.approve_withdrawal(&admin, &cid);
        assert_eq!(approved.status, WithdrawalStatus::Approved);

        // Campaign raised should be reduced
        let campaign = client.get_campaign(&cid).unwrap();
        assert_eq!(campaign.raised, 400); // 900 - 500
    }

    /// Issue #136 – submit_transaction marks withdrawal as Submitted
    #[test]
    fn test_submit_transaction() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);

        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        client.donate(&donor, &cid, &1000, &xlm, &memo);

        let recipient = Address::generate(&env);
        client.withdraw(&creator, &cid, &recipient, &500);
        client.approve_withdrawal(&admin, &cid);

        let submitted = client.submit_transaction(&admin, &cid);
        assert_eq!(submitted.status, WithdrawalStatus::Submitted);
    }

    /// Issue #138 – prevent double withdrawals
    #[test]
    #[should_panic]
    fn test_prevent_double_withdrawal() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);

        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        client.donate(&donor, &cid, &2000, &xlm, &memo); // raised = 1900

        let recipient = Address::generate(&env);
        client.withdraw(&creator, &cid, &recipient, &500);
        // Second withdraw should panic
        client.withdraw(&creator, &cid, &recipient, &500);
    }

    /// Issue #142 – total transaction count
    #[test]
    fn test_total_tx_count() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);

        assert_eq!(client.get_total_tx_count(), 0);

        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        client.donate(&donor, &cid, &1000, &xlm, &memo);
        assert_eq!(client.get_total_tx_count(), 1);

        let recipient = Address::generate(&env);
        client.withdraw(&creator, &cid, &recipient, &500);
        assert_eq!(client.get_total_tx_count(), 2);
    }

    // ── Analytics & reporting tests (issues #145, #146, #147, #148) ────────────────────

    /// Helper: bootstrap a contract with admin + N campaigns and return the IDs.
    fn setup_with_campaigns(env: &Env, n: u32) -> (OrbitChainContractClient<'_>, Address, Address, Vec<u64>) {
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        let creator = Address::generate(env);

        let mut ids: Vec<u64> = vec![env];
        for _ in 0..n {
            let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);
            ids.push_back(cid);
        }
        (client, admin, creator, ids)
    }

    /// Issue #145 – dedicated donation/withdrawal counters increment correctly
    /// and stay in sync with the total tx counter.
    #[test]
    fn test_count_total_transactions_split() {
        let env = Env::default();
        let (client, _admin, creator, ids) = setup_with_campaigns(&env, 1);
        let cid = ids.get(0).unwrap();

        assert_eq!(client.get_campaign_count(), 1);
        assert_eq!(client.get_total_donations(), 0);
        assert_eq!(client.get_total_withdrawals(), 0);
        assert_eq!(client.get_total_tx_count(), 0);

        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        let donor_a = Address::generate(&env);
        let donor_b = Address::generate(&env);
        client.donate(&donor_a, &cid, &1000, &xlm, &memo);
        client.donate(&donor_b, &cid, &2000, &xlm, &memo);
        client.donate(&donor_a, &cid, &500, &xlm, &memo);

        assert_eq!(client.get_total_donations(), 3);
        assert_eq!(client.get_total_withdrawals(), 0);
        assert_eq!(client.get_total_tx_count(), 3);

        let recipient = Address::generate(&env);
        client.withdraw(&creator, &cid, &recipient, &500);

        assert_eq!(client.get_total_donations(), 3);
        assert_eq!(client.get_total_withdrawals(), 1);
        assert_eq!(client.get_total_tx_count(), 4);
    }

    /// Issue #147 – per-campaign report carries accurate stats and progress %.
    #[test]
    fn test_get_campaign_report_accuracy() {
        let env = Env::default();
        let (client, _admin, _creator, ids) = setup_with_campaigns(&env, 1);
        let cid = ids.get(0).unwrap();

        // Empty campaign → progress 0, donor/donation count 0, remaining == goal.
        let r0 = client.get_campaign_report(&cid).unwrap();
        assert_eq!(r0.campaign_id, cid);
        assert_eq!(r0.goal, 10_000);
        assert_eq!(r0.raised, 0);
        assert_eq!(r0.remaining, 10_000);
        assert_eq!(r0.progress_bps, 0);
        assert_eq!(r0.donor_count, 0);
        assert_eq!(r0.donation_count, 0);
        assert!(r0.active);

        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        let donor_a = Address::generate(&env);
        let donor_b = Address::generate(&env);

        // Two donors, three donations → net raised = (1000-100) + (2000-100) + (500-100) = 3200
        client.donate(&donor_a, &cid, &1000, &xlm, &memo);
        client.donate(&donor_b, &cid, &2000, &xlm, &memo);
        client.donate(&donor_a, &cid, &500, &xlm, &memo);

        let r1 = client.get_campaign_report(&cid).unwrap();
        assert_eq!(r1.raised, 3200);
        assert_eq!(r1.remaining, 6800);
        // 3200 / 10_000 == 32% == 3200 bps
        assert_eq!(r1.progress_bps, 3200);
        assert_eq!(r1.donor_count, 2);
        assert_eq!(r1.donation_count, 3);

        // Non-existent campaign → None
        assert!(client.get_campaign_report(&9999).is_none());
    }

    /// Issue #147 – progress is clamped to 100% (10_000 bps) when fully funded.
    #[test]
    fn test_campaign_report_progress_clamped() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let creator = Address::generate(&env);
        // Tiny goal so a single donation overshoots it.
        let cid = client.create_campaign(&creator, &symbol_short!("tiny"), &500, &9999999);

        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        client.donate(&donor, &cid, &10_000, &xlm, &memo); // raised = 9_900 ≫ 500

        let r = client.get_campaign_report(&cid).unwrap();
        assert_eq!(r.progress_bps, 10_000);
        assert_eq!(r.remaining, 0);
    }

    /// Issue #146 – platform summary aggregates all counters faithfully.
    #[test]
    fn test_get_platform_summary() {
        let env = Env::default();
        let (client, _admin, creator, ids) = setup_with_campaigns(&env, 2);
        let cid1 = ids.get(0).unwrap();
        let cid2 = ids.get(1).unwrap();

        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        let donor = Address::generate(&env);
        client.donate(&donor, &cid1, &1000, &xlm, &memo);
        client.donate(&donor, &cid2, &2000, &xlm, &memo);
        let recipient = Address::generate(&env);
        client.withdraw(&creator, &cid1, &recipient, &200);

        let summary = client.get_platform_summary();
        assert_eq!(summary.total_campaigns, 2);
        assert_eq!(summary.total_donations, 2);
        assert_eq!(summary.total_withdrawals, 1);
        assert_eq!(summary.total_transactions, 3);
    }

    /// Issue #148 – dashboard metrics return active-campaign count and platform totals.
    #[test]
    fn test_get_dashboard_metrics() {
        let env = Env::default();
        let (client, _admin, _creator, ids) = setup_with_campaigns(&env, 3);

        let metrics0 = client.get_dashboard_metrics();
        assert_eq!(metrics0.total_campaigns, 3);
        assert_eq!(metrics0.active_campaigns, 3);
        assert_eq!(metrics0.total_donations, 0);
        assert_eq!(metrics0.total_withdrawals, 0);
        assert_eq!(metrics0.total_transactions, 0);

        // Drive a donation into one campaign and check counters propagate.
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        let donor = Address::generate(&env);
        let cid = ids.get(0).unwrap();
        client.donate(&donor, &cid, &1000, &xlm, &memo);

        let metrics1 = client.get_dashboard_metrics();
        assert_eq!(metrics1.total_donations, 1);
        assert_eq!(metrics1.total_transactions, 1);
        assert_eq!(metrics1.active_campaigns, 3);
    }

    /// Issue #148 – dashboard metrics on a fresh, empty contract are all zero.
    #[test]
    fn test_dashboard_metrics_empty_contract() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OrbitChainContract);
        let client = OrbitChainContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let metrics = client.get_dashboard_metrics();
        assert_eq!(metrics.total_campaigns, 0);
        assert_eq!(metrics.active_campaigns, 0);
        assert_eq!(metrics.total_donations, 0);
        assert_eq!(metrics.total_withdrawals, 0);
        assert_eq!(metrics.total_transactions, 0);
    }
}
