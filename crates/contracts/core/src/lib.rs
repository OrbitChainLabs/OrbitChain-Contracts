#![no_std]
use soroban_sdk::{contract, contractimpl, token, Address, Env, String, Vec};

pub mod assets;
pub mod validation;
pub mod events;
pub mod donation;
pub mod storage_optimized;
pub mod donation_optimized;
pub mod storage_tests;
pub mod rbac;
pub mod refunds;

#[contract]
pub struct CoreContract;

#[contractimpl]
impl CoreContract {
    pub fn init(env: Env, admin: Address) {
        // Initialize global admin
        rbac::Rbac::set_admin(&env, &admin);

        // Initialize asset configuration
        assets::AssetConfig::init(&env, &admin);
    }

    pub fn ping(_env: Env) -> u32 {
        1
    }

    /// Record a donation and emit the DonationReceived event
    pub fn donate(
        env: Env,
        donor: Address,
        amount: i128,
        asset: String,
        project_id: String,
        tx_hash: String,
    ) -> i128 {
        // Check for duplicate transaction
        if donation::is_transaction_processed(&env, &tx_hash) {
            // Emit rejection event for duplicate
            events::DonationRejected {
                tx_hash: tx_hash.clone(),
                reason: String::from_str(&env, "Duplicate transaction hash"),
                timestamp: env.ledger().timestamp(),
            }
            .emit(&env);
            return 0;
        }

        // Validate donation data with detailed error handling
        match donation::validate_donation_with_error(&env, &donor, amount, &asset, &project_id) {
            Ok(()) => {},
            Err(_) => return 0,
        }

        if refunds::register_donation(&env, &project_id, amount).is_err() {
            return 0;
        }

        // Get timestamp from ledger
        let timestamp = env.ledger().timestamp();

        // Mark transaction as processed BEFORE storing (prevents reentrancy)
        donation::mark_transaction_processed(&env, &tx_hash);

        // Store the donation on-chain
        let donation = donation::Donation::new(
            donor.clone(),
            amount,
            asset.clone(),
            project_id.clone(),
            timestamp,
            tx_hash.clone(),
        );
        
        // Get the index for this donation
        let index = donation::increment_donation_count(&env, &project_id) - 1;
        donation.store(&env, &project_id, index);

        // Emit the DonationReceived event with project_id
        events::DonationReceived {
            donor: donor.clone(),
            amount,
            asset: asset.clone(),
            project_id: project_id.clone(),
            timestamp,
        }
        .emit(&env);

        amount
    }

    /// Get all donations for a project
    pub fn get_donations(env: Env, project_id: String) -> soroban_sdk::Vec<Donation> {
        donation::get_donations_by_project(&env, &project_id)
    }

    pub fn upsert_campaign(
        env: Env,
        caller: Address,
        project_id: String,
        beneficiary: Address,
        goal_amount: i128,
        end_timestamp: u64,
        allow_donor_refunds: bool,
        refund_bps: u32,
        refund_deadline: u64,
    ) -> Result<refunds::Campaign, String> {
        let campaign = refunds::upsert_campaign(
            &env,
            &caller,
            &project_id,
            &beneficiary,
            goal_amount,
            end_timestamp,
            allow_donor_refunds,
            refund_bps,
            refund_deadline,
        )
        .map_err(|err| String::from_str(&env, err))?;

        events::CampaignConfigured {
            project_id,
            beneficiary,
            goal_amount,
            end_timestamp,
            timestamp: env.ledger().timestamp(),
        }
        .emit(&env);

        Ok(campaign)
    }

    pub fn get_campaign(env: Env, project_id: String) -> Option<refunds::Campaign> {
        refunds::get_campaign(&env, &project_id)
    }

    pub fn cancel_campaign(
        env: Env,
        caller: Address,
        project_id: String,
        reason: String,
    ) -> Result<refunds::Campaign, String> {
        let campaign = refunds::cancel_campaign(&env, &caller, &project_id, &reason)
            .map_err(|err| String::from_str(&env, err))?;

        events::CampaignCancelled {
            project_id,
            cancelled_by: caller,
            reason,
            timestamp: env.ledger().timestamp(),
        }
        .emit(&env);

        Ok(campaign)
    }

    pub fn request_refund(
        env: Env,
        donor: Address,
        project_id: String,
        donation_index: u32,
    ) -> Result<refunds::RefundRequest, String> {
        let request = refunds::request_refund(&env, &donor, &project_id, donation_index)
            .map_err(|err| String::from_str(&env, err))?;

        events::RefundRequested {
            donor,
            project_id,
            donation_index,
            refundable_amount: request.refundable_amount,
            asset: request.asset.clone(),
            timestamp: request.requested_at,
        }
        .emit(&env);

        Ok(request)
    }

    pub fn process_refund(
        env: Env,
        caller: Address,
        project_id: String,
        donation_index: u32,
        approve: bool,
        reason: String,
    ) -> Result<refunds::RefundRequest, String> {
        let request = refunds::process_refund(
            &env,
            &caller,
            &project_id,
            donation_index,
            approve,
            &reason,
        )
        .map_err(|err| String::from_str(&env, err))?;

        if approve {
            events::RefundApproved {
                processor: caller.clone(),
                donor: request.donor.clone(),
                project_id: project_id.clone(),
                donation_index,
                refundable_amount: request.refundable_amount,
                timestamp: request.updated_at,
            }
            .emit(&env);

            events::RefundProcessed {
                processor: caller,
                donor: request.donor.clone(),
                project_id,
                donation_index,
                refundable_amount: request.refundable_amount,
                asset: request.asset.clone(),
                timestamp: request.updated_at,
            }
            .emit(&env);
        } else {
            events::RefundRejected {
                processor: caller,
                donor: request.donor.clone(),
                project_id,
                donation_index,
                reason,
                timestamp: request.updated_at,
            }
            .emit(&env);
        }

        Ok(request)
    }

    pub fn batch_refund(
        env: Env,
        caller: Address,
        project_id: String,
        donation_indices: Vec<u32>,
    ) -> Result<u32, String> {
        let processed = refunds::batch_refund(&env, &caller, &project_id, &donation_indices)
            .map_err(|err| String::from_str(&env, err))?;

        events::BatchRefundProcessed {
            processor: caller,
            project_id,
            processed_count: processed,
            timestamp: env.ledger().timestamp(),
        }
        .emit(&env);

        Ok(processed)
    }

    pub fn get_refund_status(
        env: Env,
        project_id: String,
        donation_index: u32,
    ) -> Option<refunds::RefundStatus> {
        refunds::get_refund_status(&env, &project_id, donation_index)
    }

    pub fn get_eligible_refunds(
        env: Env,
        donor: Address,
        project_id: String,
    ) -> Vec<refunds::EligibleRefund> {
        refunds::get_eligible_refunds(&env, &donor, &project_id)
    }

    // ===== Asset Management Functions (Admin Only) =====

    /// Add a new supported asset (admin only)
    pub fn add_supported_asset(
        env: Env,
        caller: Address,
        asset_code: String,
        contract_address: Address,
    ) -> Result<String, String> {
        // Ensure caller is admin
        rbac::Rbac::require_admin_auth(&env, &caller);

        assets::AssetConfig::add_asset(&env, &caller, &asset_code.to_string(), contract_address)
            .map(|_| asset_code)
            .map_err(|e| String::from_str(&env, e))
    }

    /// Remove a supported asset (admin only)
    pub fn remove_supported_asset(env: Env, caller: Address, asset_code: String) -> Result<String, String> {
        assets::AssetConfig::remove_asset(&env, &caller, &asset_code.to_string())
            .map(|_| asset_code)
            .map_err(|e| String::from_str(&env, e))
    }

    /// Update the asset admin (admin only)
    pub fn update_asset_admin(env: Env, caller: Address, new_admin: Address) -> Result<String, String> {
        assets::AssetConfig::update_admin(&env, &caller, &new_admin)
            .map(|_| String::from_str(&env, "Admin updated"))
            .map_err(|e| String::from_str(&env, e))
    }

    /// Get the list of all supported assets
    pub fn get_supported_assets(env: Env) -> soroban_sdk::Vec<String> {
        assets::AssetConfig::get_supported_assets(&env)
    }

    /// Check if an asset is supported
    pub fn is_asset_supported(env: Env, asset_code: String) -> bool {
        assets::AssetConfig::is_asset_supported(&env, &asset_code.to_string())
    }

    /// Get the current asset admin
    pub fn get_asset_admin(env: Env) -> Option<Address> {
        assets::AssetConfig::get_admin(&env)
    }

    /// Process a withdrawal and emit the WithdrawalProcessed event
    pub fn withdraw(
        env: Env,
        recipient: Address,
        amount: i128,
        asset: String,
    ) -> i128 {
        // Restricted to admin only
        rbac::Rbac::require_admin(&env);

        // Validate amount
        if amount <= 0 {
            panic!("Withdrawal amount must be positive");
        }

        // Resolve asset contract address
        let asset_code_str = asset.to_string();
        let asset_contract = assets::AssetConfig::get_contract_address(&env, &asset_code_str)
            .unwrap_or_else(|| panic!("Asset contract address not configured for {}", asset_code_str));

        // Initialize token client
        let token_client = token::Client::new(&env, &asset_contract);

        // Check contract balance
        let contract_address = env.current_contract_address();
        let balance = token_client.balance(&contract_address);
        if balance < amount {
            panic!("Insufficient contract balance for withdrawal");
        }

        // Execute transfer
        token_client.transfer(&contract_address, &recipient, &amount);

        // Emit the WithdrawalProcessed event
        events::WithdrawalProcessed {
            recipient: recipient.clone(),
            amount,
            asset: asset.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .emit(&env);

        amount
    }
}

pub use donation::Donation;
pub use refunds::{Campaign, CampaignStatus, EligibleRefund, RefundEligibility, RefundRequest, RefundStatus};

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_init_and_ping() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let result = client.ping();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_address_validation_integration() {
        use crate::validation::*;

        let env = Env::default();
        let valid_address = soroban_sdk::String::from_str(
            &env,
            "GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37",
        );

        // Test that validation utilities are accessible
        let result = validate_stellar_address(&env, valid_address);
        assert!(result.is_ok());

        // Test boolean validation
        let valid_address2 = soroban_sdk::String::from_str(
            &env,
            "GAYOLLLUIZE4DZMBB2ZBKGBUBZLIOYU6XFLW37GBP2VZD3ABNXCW4BVA",
        );
        assert!(is_valid_stellar_address(&env, valid_address2));
    }

    // ===== Project ID Donation Mapping Tests =====

    #[test]
    fn test_donate_with_valid_project_id() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, "proj-123");
        let tx_hash = String::from_str(&env, "abc123");

        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, amount);
    }

    #[test]
    fn test_donate_with_invalid_project_id_empty() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, ""); // Empty project ID
        let tx_hash = String::from_str(&env, "abc123");

        // Should return 0 for invalid project_id
        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_get_donations_groups_by_project_id() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);
        let donor3 = Address::generate(&env);

        // Donate to project A
        let project_a = String::from_str(&env, "project-alpha");
        client.donate(&donor1, &1000i128, &String::from_str(&env, "XLM"), &project_a, &String::from_str(&env, "tx1"));
        client.donate(&donor2, &2000i128, &String::from_str(&env, "USDC"), &project_a, &String::from_str(&env, "tx2"));

        // Donate to project B
        let project_b = String::from_str(&env, "project-beta");
        client.donate(&donor3, &500i128, &String::from_str(&env, "XLM"), &project_b, &String::from_str(&env, "tx3"));

        // Get donations for project A
        let donations_a = client.get_donations(&project_a);
        assert_eq!(donations_a.len(), 2);

        // Get donations for project B
        let donations_b = client.get_donations(&project_b);
        assert_eq!(donations_b.len(), 1);
    }

    #[test]
    fn test_duplicate_transaction_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let project_id = String::from_str(&env, "test-project");
        let tx_hash = String::from_str(&env, "unique-tx-hash-123");

        // First donation should succeed
        let result1 = client.donate(&donor, &1000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash);
        assert_eq!(result1, 1000i128);

        // Second donation with same tx_hash should be rejected
        let result2 = client.donate(&donor, &2000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash);
        assert_eq!(result2, 0);
    }

    // ===== Admin & Asset Management Tests =====

    #[test]
    fn test_admin_add_supported_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Admin adds BTC
        let btc_address = Address::generate(&env);
        let result = client.add_supported_asset(&admin, &String::from_str(&env, "BTC"), &btc_address);
        assert!(result.is_ok());

        assert!(client.is_asset_supported(&String::from_str(&env, "BTC")));
    }

    #[test]
    fn test_non_admin_cannot_add_asset() {
        let env = Env::default();
        // No mock_all_auths to test failure
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        client.init(&admin);

        let btc_address = Address::generate(&env);
        let result = client.add_supported_asset(&other, &String::from_str(&env, "BTC"), &btc_address);
        assert!(result.is_err());
    }

    #[test]
    fn test_withdraw_success() {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let asset_code = String::from_str(&env, "USDC");
        let asset_contract = env.register_stellar_asset_contract(Address::generate(&env));
        client.add_supported_asset(&admin, &asset_code, &asset_contract);

        // Fund contract
        let amount = 1000i128;
        let token_admin = token::StellarAssetClient::new(&env, &asset_contract);
        token_admin.mint(&contract_id, &amount);

        let recipient = Address::generate(&env);
        let withdraw_amount = 500i128;
        let result = client.withdraw(&recipient, &withdraw_amount, &asset_code);

        assert_eq!(result, withdraw_amount);
        let token_client = token::Client::new(&env, &asset_contract);
        assert_eq!(token_client.balance(&contract_id), 500i128);
        assert_eq!(token_client.balance(&recipient), 500i128);
    }

    #[test]
    #[should_panic(expected = "Insufficient contract balance for withdrawal")]
    fn test_withdraw_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let asset_code = String::from_str(&env, "USDC");
        let asset_contract = env.register_stellar_asset_contract(Address::generate(&env));
        client.add_supported_asset(&admin, &asset_code, &asset_contract);

        let recipient = Address::generate(&env);
        client.withdraw(&recipient, &1000i128, &asset_code);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: caller is not admin")]
    fn test_withdraw_unauthorized() {
        let env = Env::default();
        
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let recipient = Address::generate(&env);
        client.withdraw(&recipient, &1000i128, &String::from_str(&env, "USDC"));
    }

    #[test]
    fn test_cancelled_campaign_refund_flow() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let beneficiary = Address::generate(&env);
        let donor = Address::generate(&env);
        client.init(&admin);

        let asset_code = String::from_str(&env, "USDC");
        let asset_contract = env.register_stellar_asset_contract(Address::generate(&env));
        client.add_supported_asset(&admin, &asset_code, &asset_contract);

        client.upsert_campaign(
            &admin,
            &String::from_str(&env, "campaign-refund"),
            &beneficiary,
            &10_000i128,
            &500u64,
            &false,
            &10_000u32,
            &800u64,
        );

        client.donate(
            &donor,
            &2_500i128,
            &asset_code,
            &String::from_str(&env, "campaign-refund"),
            &String::from_str(&env, "refund-flow-tx"),
        );

        let token_admin = token::StellarAssetClient::new(&env, &asset_contract);
        token_admin.mint(&contract_id, &2_500i128);

        client.cancel_campaign(
            &admin,
            &String::from_str(&env, "campaign-refund"),
            &String::from_str(&env, "creator cancelled"),
        );

        let request = client.request_refund(&donor, &String::from_str(&env, "campaign-refund"), &0u32);
        assert_eq!(request.status, RefundStatus::Pending);

        let completed = client.process_refund(
            &beneficiary,
            &String::from_str(&env, "campaign-refund"),
            &0u32,
            &true,
            &String::from_str(&env, ""),
        );
        assert_eq!(completed.status, RefundStatus::Completed);

        let token_client = token::Client::new(&env, &asset_contract);
        assert_eq!(token_client.balance(&donor), 2_500i128);
        assert_eq!(client.get_refund_status(&String::from_str(&env, "campaign-refund"), &0u32), Some(RefundStatus::Completed));
    }

    #[test]
    fn test_get_eligible_refunds_time_locked_before_campaign_end() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let beneficiary = Address::generate(&env);
        let donor = Address::generate(&env);
        client.init(&admin);

        client.upsert_campaign(
            &admin,
            &String::from_str(&env, "timelock-campaign"),
            &beneficiary,
            &10_000i128,
            &1_000u64,
            &true,
            &10_000u32,
            &1_500u64,
        );

        client.donate(
            &donor,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &String::from_str(&env, "timelock-campaign"),
            &String::from_str(&env, "timelock-tx"),
        );

        let eligible = client.get_eligible_refunds(&donor, &String::from_str(&env, "timelock-campaign"));
        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible.get(0).unwrap().eligibility, RefundEligibility::TimeLocked);
    }

    #[test]
    fn test_partial_refund_and_batch_refund() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let beneficiary = Address::generate(&env);
        let donor_one = Address::generate(&env);
        let donor_two = Address::generate(&env);
        client.init(&admin);

        let asset_code = String::from_str(&env, "USDC");
        let asset_contract = env.register_stellar_asset_contract(Address::generate(&env));
        client.add_supported_asset(&admin, &asset_code, &asset_contract);

        client.upsert_campaign(
            &admin,
            &String::from_str(&env, "batch-campaign"),
            &beneficiary,
            &50_000i128,
            &500u64,
            &false,
            &5_000u32,
            &900u64,
        );

        client.donate(
            &donor_one,
            &4_000i128,
            &asset_code,
            &String::from_str(&env, "batch-campaign"),
            &String::from_str(&env, "batch-tx-1"),
        );
        client.donate(
            &donor_two,
            &6_000i128,
            &asset_code,
            &String::from_str(&env, "batch-campaign"),
            &String::from_str(&env, "batch-tx-2"),
        );

        let token_admin = token::StellarAssetClient::new(&env, &asset_contract);
        token_admin.mint(&contract_id, &5_000i128);

        client.cancel_campaign(
            &admin,
            &String::from_str(&env, "batch-campaign"),
            &String::from_str(&env, "cancelled for refunds"),
        );

        let donor_one_refunds = client.get_eligible_refunds(&donor_one, &String::from_str(&env, "batch-campaign"));
        assert_eq!(donor_one_refunds.get(0).unwrap().eligibility, RefundEligibility::PartiallyEligible);
        assert_eq!(donor_one_refunds.get(0).unwrap().refundable_amount, 2_000i128);

        let mut donation_indices = Vec::new(&env);
        donation_indices.push_back(0u32);
        donation_indices.push_back(1u32);

        let processed = client.batch_refund(&beneficiary, &String::from_str(&env, "batch-campaign"), &donation_indices);
        assert_eq!(processed, 2u32);

        let token_client = token::Client::new(&env, &asset_contract);
        assert_eq!(token_client.balance(&donor_one), 2_000i128);
        assert_eq!(token_client.balance(&donor_two), 3_000i128);
    }

    #[test]
    fn test_expired_campaign_without_goal_allows_refund_request() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let beneficiary = Address::generate(&env);
        let donor = Address::generate(&env);
        client.init(&admin);

        client.upsert_campaign(
            &admin,
            &String::from_str(&env, "expired-campaign"),
            &beneficiary,
            &10_000i128,
            &100u64,
            &false,
            &10_000u32,
            &300u64,
        );

        client.donate(
            &donor,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &String::from_str(&env, "expired-campaign"),
            &String::from_str(&env, "expired-tx"),
        );

        env.ledger().with_mut(|li| {
            li.timestamp = 150u64;
        });

        let request = client.request_refund(&donor, &String::from_str(&env, "expired-campaign"), &0u32);
        assert_eq!(request.status, RefundStatus::Pending);
    }

    #[test]
    fn test_pending_refund_becomes_expired_after_deadline() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let beneficiary = Address::generate(&env);
        let donor = Address::generate(&env);
        client.init(&admin);

        client.upsert_campaign(
            &admin,
            &String::from_str(&env, "expiry-status-campaign"),
            &beneficiary,
            &10_000i128,
            &100u64,
            &true,
            &10_000u32,
            &160u64,
        );

        client.donate(
            &donor,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &String::from_str(&env, "expiry-status-campaign"),
            &String::from_str(&env, "expiry-status-tx"),
        );

        env.ledger().with_mut(|li| {
            li.timestamp = 120u64;
        });

        client.request_refund(&donor, &String::from_str(&env, "expiry-status-campaign"), &0u32);

        env.ledger().with_mut(|li| {
            li.timestamp = 200u64;
        });

        assert_eq!(
            client.get_refund_status(&String::from_str(&env, "expiry-status-campaign"), &0u32),
            Some(RefundStatus::Expired)
        );
    }
}
