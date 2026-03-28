#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, String};

pub mod assets;
pub mod validation;
pub mod events;
pub mod donation;
pub mod storage_optimized;
pub mod donation_optimized;
pub mod storage_tests;
pub mod rbac;

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
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `donor` - The address of the donor
    /// * `amount` - The amount donated
    /// * `asset` - The asset type donated (e.g., "XLM", "USDC")
    /// * `project_id` - The project ID to map this donation to (3-64 chars, alphanumeric with hyphens/underscores)
    /// * `tx_hash` - The transaction hash of the donation (must be unique)
    ///
    /// # Returns
    /// * The donation amount if successful
    /// * 0 if validation fails or duplicate transaction (check validation error for details)
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

    // ===== Asset Management Functions (Admin Only) =====

    /// Add a new supported asset (admin only)
    pub fn add_supported_asset(env: Env, caller: Address, asset_code: String) -> Result<String, String> {
        assets::AssetConfig::add_asset(&env, &caller, &asset_code)
            .map_err(|e| String::from_str(&env, e))
    }

    /// Remove a supported asset (admin only)
    pub fn remove_supported_asset(env: Env, caller: Address, asset_code: String) -> Result<String, String> {
        assets::AssetConfig::remove_asset(&env, &caller, &asset_code)
            .map_err(|e| String::from_str(&env, e))
    }

    /// Update the asset admin (admin only)
    pub fn update_asset_admin(env: Env, caller: Address, new_admin: Address) -> Result<String, String> {
        assets::AssetConfig::update_admin(&env, &caller, &new_admin)
            .map_err(|e| String::from_str(&env, e))
    }

    /// Get the list of all supported assets
    pub fn get_supported_assets(env: Env) -> soroban_sdk::Vec<String> {
        assets::AssetConfig::get_supported_assets(&env)
    }

    /// Check if an asset is supported
    pub fn is_asset_supported(env: Env, asset_code: String) -> bool {
        assets::AssetConfig::is_asset_supported(&env, &asset_code)
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
    fn test_donate_with_invalid_project_id_too_short() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, "AB"); // Too short (min 3 chars)
        let tx_hash = String::from_str(&env, "abc123");

        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_donate_with_invalid_project_id_invalid_chars() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, "proj@123"); // Invalid character @
        let tx_hash = String::from_str(&env, "abc123");

        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_donate_with_invalid_project_id_starts_with_hyphen() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, "-proj123"); // Starts with hyphen
        let tx_hash = String::from_str(&env, "abc123");

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

        // Verify amounts
        assert_eq!(donations_a.get(0).unwrap().amount, 1000i128);
        assert_eq!(donations_a.get(1).unwrap().amount, 2000i128);
        assert_eq!(donations_b.get(0).unwrap().amount, 500i128);
    }

    #[test]
    fn test_donation_project_id_mapping_integrity() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let project_id = String::from_str(&env, "test-project-001");
        let tx_hash = String::from_str(&env, "txhash123");

        // Make donation
        client.donate(&donor, &1500i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash);

        // Retrieve and verify project_id is correctly stored
        let donations = client.get_donations(&project_id);
        assert_eq!(donations.len(), 1);
        
        let stored_donation = donations.get(0).unwrap();
        assert_eq!(stored_donation.project_id, project_id);
        assert_eq!(stored_donation.amount, 1500i128);
        assert_eq!(stored_donation.donor, donor);
    }

    #[test]
    fn test_multiple_projects_isolation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);

        // Create multiple projects
        let projects = vec![
            String::from_str(&env, "proj-001"),
            String::from_str(&env, "proj-002"),
            String::from_str(&env, "proj-003"),
        ];

        // Donate to each project
        for (i, project_id) in projects.iter().enumerate() {
            let amount = ((i + 1) * 100) as i128;
            let tx_hash = String::from_str(&env, &format!("tx{}", i));
            client.donate(&donor, &amount, &String::from_str(&env, "XLM"), &project_id, &tx_hash);
        }

        // Verify each project has exactly one donation
        for project_id in projects.iter() {
            let donations = client.get_donations(&project_id);
            assert_eq!(donations.len(), 1, "Project should have exactly one donation");
        }
    }

    // ===== Duplicate Transaction Prevention Tests =====

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

        // Verify only one donation was recorded
        let donations = client.get_donations(&project_id);
        assert_eq!(donations.len(), 1);
        assert_eq!(donations.get(0).unwrap().amount, 1000i128);
    }

    #[test]
    fn test_different_transactions_same_project_allowed() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let project_id = String::from_str(&env, "test-project");

        // Multiple donations with different tx_hashes should all succeed
        let tx_hash1 = String::from_str(&env, "tx-hash-001");
        let result1 = client.donate(&donor, &1000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash1);
        assert_eq!(result1, 1000i128);

        let tx_hash2 = String::from_str(&env, "tx-hash-002");
        let result2 = client.donate(&donor, &2000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash2);
        assert_eq!(result2, 2000i128);

        let tx_hash3 = String::from_str(&env, "tx-hash-003");
        let result3 = client.donate(&donor, &3000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash3);
        assert_eq!(result3, 3000i128);

        // Verify all three donations were recorded
        let donations = client.get_donations(&project_id);
        assert_eq!(donations.len(), 3);
    }

    #[test]
    fn test_same_tx_hash_different_projects_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let tx_hash = String::from_str(&env, "shared-tx-hash");

        // First donation to project A
        let project_a = String::from_str(&env, "project-a");
        let result1 = client.donate(&donor, &1000i128, &String::from_str(&env, "XLM"), &project_a, &tx_hash);
        assert_eq!(result1, 1000i128);

        // Same tx_hash to project B should be rejected
        let project_b = String::from_str(&env, "project-b");
        let result2 = client.donate(&donor, &2000i128, &String::from_str(&env, "XLM"), &project_b, &tx_hash);
        assert_eq!(result2, 0);

        // Verify only project A has the donation
        let donations_a = client.get_donations(&project_a);
        assert_eq!(donations_a.len(), 1);

        let donations_b = client.get_donations(&project_b);
        assert_eq!(donations_b.len(), 0);
    }

    #[test]
    fn test_no_double_counting_on_duplicate() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let project_id = String::from_str(&env, "funding-project");
        let tx_hash = String::from_str(&env, "double-spend-attempt");

        // Initial donation
        client.donate(&donor, &5000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash);

        // Attempt to double-spend with same tx_hash
        for _ in 0..5 {
            let result = client.donate(&donor, &5000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash);
            assert_eq!(result, 0, "Duplicate should be rejected");
        }

        // Verify total is exactly 5000 (no double counting)
        let donations = client.get_donations(&project_id);
        assert_eq!(donations.len(), 1);
        
        let total: i128 = donations.iter().map(|d| d.amount).sum();
        assert_eq!(total, 5000i128, "Total should be exactly 5000, no double counting");
    }

    #[test]
    fn test_transaction_hash_isolation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);
        let project_id = String::from_str(&env, "shared-project");

        // Donor 1 makes donation
        let tx_hash1 = String::from_str(&env, "donor1-tx");
        client.donate(&donor1, &1000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash1);

        // Donor 2 tries to use same tx_hash (should fail)
        let result = client.donate(&donor2, &2000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash1);
        assert_eq!(result, 0);

        // Donor 2 with different tx_hash should succeed
        let tx_hash2 = String::from_str(&env, "donor2-tx");
        let result2 = client.donate(&donor2, &2000i128, &String::from_str(&env, "XLM"), &project_id, &tx_hash2);
        assert_eq!(result2, 2000i128);

        // Verify both donations recorded
        let donations = client.get_donations(&project_id);
        assert_eq!(donations.len(), 2);
    }

    #[test]
    fn test_system_consistency_after_duplicate_attempt() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let project_id = String::from_str(&env, "consistency-test");

        // Series of valid donations
        let hashes = vec![
            String::from_str(&env, "tx-001"),
            String::from_str(&env, "tx-002"),
            String::from_str(&env, "tx-003"),
        ];

        for (i, tx_hash) in hashes.iter().enumerate() {
            let amount = ((i + 1) * 1000) as i128;
            client.donate(&donor, &amount, &String::from_str(&env, "XLM"), &project_id, &tx_hash);
        }

        // Attempt duplicate of middle transaction
        let duplicate_result = client.donate(&donor, &9999i128, &String::from_str(&env, "XLM"), &project_id, &hashes.get(1).unwrap());
        assert_eq!(duplicate_result, 0);

        // Verify system state is consistent
        let donations = client.get_donations(&project_id);
        assert_eq!(donations.len(), 3);

        // Verify amounts are unchanged
        assert_eq!(donations.get(0).unwrap().amount, 1000i128);
        assert_eq!(donations.get(1).unwrap().amount, 2000i128);
        assert_eq!(donations.get(2).unwrap().amount, 3000i128);

        // Verify total
        let total: i128 = donations.iter().map(|d| d.amount).sum();
        assert_eq!(total, 6000i128);
    }

    // ===== Asset Validation Tests =====

    #[test]
    fn test_donate_with_supported_asset_xlm() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "XLM");
        let project_id = String::from_str(&env, "test-project");
        let tx_hash = String::from_str(&env, "tx-xlm-001");

        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, amount);
    }

    #[test]
    fn test_donate_with_supported_asset_usdc() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 5000i128;
        let asset = String::from_str(&env, "USDC");
        let project_id = String::from_str(&env, "test-project");
        let tx_hash = String::from_str(&env, "tx-usdc-001");

        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, amount);
    }

    #[test]
    fn test_donate_with_unsupported_asset_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, "BTC"); // Not supported by default
        let project_id = String::from_str(&env, "test-project");
        let tx_hash = String::from_str(&env, "tx-btc-001");

        // Should return 0 for unsupported asset
        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_donate_with_empty_asset_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let amount = 1000i128;
        let asset = String::from_str(&env, ""); // Empty asset
        let project_id = String::from_str(&env, "test-project");
        let tx_hash = String::from_str(&env, "tx-empty-001");

        let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_admin_add_supported_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Initially BTC should not be supported
        assert!(!client.is_asset_supported(&String::from_str(&env, "BTC")));

        // Admin adds BTC
        let result = client.add_supported_asset(&admin, &String::from_str(&env, "BTC"));
        assert!(result.is_ok());

        // Now BTC should be supported
        assert!(client.is_asset_supported(&String::from_str(&env, "BTC")));
    }

    #[test]
    fn test_non_admin_cannot_add_asset() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        client.init(&admin);

        // Non-admin tries to add asset
        let result = client.add_supported_asset(&other, &String::from_str(&env, "BTC"));
        assert!(result.is_err());
    }

    #[test]
    fn test_admin_remove_supported_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // EURT is initially supported
        assert!(client.is_asset_supported(&String::from_str(&env, "EURT")));

        // Admin removes EURT
        let result = client.remove_supported_asset(&admin, &String::from_str(&env, "EURT"));
        assert!(result.is_ok());

        // Now EURT should not be supported
        assert!(!client.is_asset_supported(&String::from_str(&env, "EURT")));
    }

    #[test]
    fn test_get_supported_assets() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let assets = client.get_supported_assets();
        assert_eq!(assets.len(), 5);

        // Verify all default assets are present
        assert!(assets.contains(&String::from_str(&env, "XLM")));
        assert!(assets.contains(&String::from_str(&env, "USDC")));
        assert!(assets.contains(&String::from_str(&env, "NGNT")));
        assert!(assets.contains(&String::from_str(&env, "USDT")));
        assert!(assets.contains(&String::from_str(&env, "EURT")));
    }

    #[test]
    fn test_donation_with_all_supported_assets() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let donor = Address::generate(&env);
        let project_id = String::from_str(&env, "multi-asset-project");

        // Test all supported assets
        let assets = vec![
            String::from_str(&env, "XLM"),
            String::from_str(&env, "USDC"),
            String::from_str(&env, "NGNT"),
            String::from_str(&env, "USDT"),
            String::from_str(&env, "EURT"),
        ];

        for (i, asset) in assets.iter().enumerate() {
            let amount = ((i + 1) * 1000) as i128;
            let tx_hash = String::from_str(&env, &format!("tx-{}", i));
            let result = client.donate(&donor, &amount, &asset, &project_id, &tx_hash);
            assert_eq!(result, amount, "Donation with asset {:?} should succeed", asset);
        }

        // Verify all donations were recorded
        let donations = client.get_donations(&project_id);
        assert_eq!(donations.len(), 5);
    }

    #[test]
    fn test_asset_admin_update() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        client.init(&admin);

        // Verify initial admin
        assert_eq!(client.get_asset_admin(), Some(admin.clone()));

        // Update admin
        let result = client.update_asset_admin(&admin, &new_admin);
        assert!(result.is_ok());

        // Verify new admin
        assert_eq!(client.get_asset_admin(), Some(new_admin.clone()));

        // New admin should have permissions
        let new_admin_result = client.add_supported_asset(&new_admin, &String::from_str(&env, "BTC"));
        assert!(new_admin_result.is_ok());
    }

    #[test]
    fn test_withdraw_admin_only() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let recipient = Address::generate(&env);
        let amount = 5000i128;
        let asset = String::from_str(&env, "USDC");

        // Admin call should succeed (mock_all_auths handles the auth check)
        let result = client.withdraw(&recipient, &amount, &asset);
        assert_eq!(result, amount);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: caller is not admin")]
    fn test_non_admin_cannot_add_asset_v2() {
        let env = Env::default();
        // env.mock_all_auths(); // Don't mock all auths to test unauthorized access
        let contract_id = env.register_contract(None, CoreContract);
        let client = CoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        client.init(&admin);

        // Non-admin tries to add asset - should panic now instead of returning Err
        client.add_supported_asset(&other, &String::from_str(&env, "BTC"));
    }
}
