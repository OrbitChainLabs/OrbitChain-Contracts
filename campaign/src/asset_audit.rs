//! Token contract verification helpers (issues #106, #108).
//!
//! Provides [`verify_token_contract`] which validates that an address is a
//! real SEP-41 token by probing its `name`, `symbol`, and `decimals`, and
//! [`verify_asset_metadata`] which cross-checks a token's on-chain symbol
//! against an expected asset code.

use soroban_sdk::token;
use soroban_sdk::{Address, Env, String};

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

/// Issue #108 — Cross-check a token contract's on-chain `symbol` against the
/// expected `asset_code`.
///
/// Returns `true` when the token's symbol matches the supplied asset code
/// (case-insensitive comparison).  Returns `false` when the contract does not
/// respond or the symbols don't match — this is a non-panicking view helper
/// suitable for off-chain validation.
pub fn verify_asset_metadata(env: &Env, contract: &Address, code: &String) -> bool {
    let client = token::Client::new(env, contract);
    let symbol = match client.try_symbol() {
        Ok(Ok(sym)) => sym,
        _ => return false,
    };

    bytes_eq_case_insensitive(&symbol.to_bytes(), &code.to_bytes())
}

/// Compare two `Bytes` slices case-insensitively for ASCII letters.
fn bytes_eq_case_insensitive(a: &soroban_sdk::Bytes, b: &soroban_sdk::Bytes) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        // SAFETY: i < a.len() and len already checked equal; unwrap never fails.
        let mut ba = a.get(i).unwrap();
        let mut bb = b.get(i).unwrap();
        if ba.is_ascii_lowercase() {
            ba -= b'a' - b'A';
        }
        if bb.is_ascii_lowercase() {
            bb -= b'a' - b'A';
        }
        if ba != bb {
            return false;
        }
    }
    true
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

    // ── verify_asset_metadata ───────────────────────────────────────────

    #[test]
    fn verify_asset_metadata_returns_false_for_missing_contract() {
        let env = Env::default();
        let addr = dead_address(&env);
        let code = String::from_str(&env, "USDC");
        assert!(!verify_asset_metadata(&env, &addr, &code));
    }

    #[test]
    fn verify_asset_metadata_returns_false_for_mismatched_code() {
        let env = Env::default();
        let token_addr = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        let wrong_code = String::from_str(&env, "WRONG");
        assert!(!verify_asset_metadata(&env, &token_addr, &wrong_code));
    }

    // ── bytes_eq_case_insensitive ───────────────────────────────────────

    #[test]
    fn case_insensitive_eq_works() {
        let env = Env::default();
        let a = String::from_str(&env, "uSdC").to_bytes();
        let b = String::from_str(&env, "USDC").to_bytes();
        assert!(bytes_eq_case_insensitive(&a, &b));
        let c = String::from_str(&env, "EURC").to_bytes();
        assert!(!bytes_eq_case_insensitive(&a, &c));
        let d = String::from_str(&env, "usdcx").to_bytes();
        assert!(!bytes_eq_case_insensitive(&a, &d));
    }
}
