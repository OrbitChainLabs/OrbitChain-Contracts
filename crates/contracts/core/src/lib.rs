#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, String};

pub mod assets;
pub mod validation;
pub mod events;

#[contract]
pub struct CoreContract;

#[contractimpl]
impl CoreContract {
    pub fn init(_env: Env, _admin: Address) {}

    pub fn ping(_env: Env) -> u32 {
        1
    }

    /// Record a donation and emit the DonationReceived event
    pub fn donate(
        env: Env,
        donor: Address,
        amount: i128,
        asset: String,
    ) -> i128 {
        // Emit the DonationReceived event
        events::DonationReceived {
            donor: donor.clone(),
            amount,
            asset: asset.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .emit(&env);

        amount
    }

    /// Process a withdrawal and emit the WithdrawalProcessed event
    pub fn withdraw(
        env: Env,
        recipient: Address,
        amount: i128,
        asset: String,
    ) -> i128 {
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
}
