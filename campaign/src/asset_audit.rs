//! Token contract verification helpers (issue #106).
//!
//! Provides [`verify_token_contract`] which validates that an address is a
//! real SEP-41 token by probing its `name`, `symbol`, and `decimals`.

use soroban_sdk::token;
use soroban_sdk::{Address, Env};

use crate::types::Error;

/// Issue #106 — Verify that `address` is a valid SEP-41 token contract.
///
/// Probes `name()`, `symbol()`, and `decimals()` on the token client.  Any
/// host error (missing contract, wrong interface, trap) is caught and surfaced
/// as `Error::InvalidTokenContract`.  A successful return means the address
/// points to a deployed contract that implements the SEP-41 interface with at
/// least the three mandatory read-only functions.
///
/// # Errors
/// Returns `Error::InvalidTokenContract` if the contract does not respond
/// to all three SEP-41 metadata calls.
pub fn verify_token_contract(env: &Env, address: &Address) -> Result<(), Error> {
    let client = token::Client::new(env, address);

    // Soroban SDK 26.x `try_*` returns `Result<Result<T, …>, Result<…, …>>`.
    // Both layers must succeed for the address to be a valid SEP-41 token.
    let _name = match client.try_name() {
        Ok(Ok(name)) => name,
        _ => return Err(Error::InvalidTokenContract),
    };
    let _symbol = match client.try_symbol() {
        Ok(Ok(sym)) => sym,
        _ => return Err(Error::InvalidTokenContract),
    };
    let _decimals = match client.try_decimals() {
        Ok(Ok(dec)) => dec,
        _ => return Err(Error::InvalidTokenContract),
    };

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    /// A Soroban address that has no deployed contract at all.
    /// `token::Client::new` will succeed but every `try_*` call will return
    /// `Err`.
    fn dead_address(env: &Env) -> Address {
        Address::generate(env)
    }

    #[test]
    fn verify_token_contract_fails_on_invalid_address() {
        let env = Env::default();
        let bad = dead_address(&env);
        let result = verify_token_contract(&env, &bad);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::InvalidTokenContract);
    }

    #[test]
    fn verify_token_contract_succeeds_on_registered_token() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let token_addr = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let result = verify_token_contract(&env, &token_addr);
        assert!(result.is_ok());
    }
}
