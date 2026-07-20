//! Issue #57 — `bump-storage` CLI wiring for the operator-only
//! `bump_storage_as_operator` contract entrypoint.
//!
//! Named `bump_storage_as_operator` rather than `bump_storage` because issue
//! #120 already shipped a `bump_storage(env)` that is deliberately public
//! and requires no auth. Both entrypoints exist on the contract; see the PR
//! description for the resulting overlap.
//!
//! This binary does not embed a Soroban RPC client (see docs/deployment.md
//! "Known Limitations"), so — like `scripts/deploy.sh` — real on-chain
//! invocation is delegated to the `stellar` CLI. This module's own
//! responsibility is deriving the operator's public address from its secret
//! key so it can be passed as the contract's `--operator` argument.

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use std::process::Command;
use stellar_strkey::ed25519::{PrivateKey, PublicKey};

use crate::key_manager::KeyManager;

/// Derive the Stellar public address (`G...`) for a given secret key (`S...`).
pub fn derive_public_key(secret_key: &str) -> Result<String> {
    let seed = PrivateKey::from_string(secret_key)
        .map_err(|_| anyhow::anyhow!("Invalid Stellar secret key encoding"))?
        .0;
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key = PublicKey(signing_key.verifying_key().to_bytes());
    Ok(format!("{}", public_key))
}

/// Invoke the operator-only `bump_storage_as_operator` entrypoint on a
/// deployed campaign contract, refreshing the TTL of every core persistent
/// key plus every milestone so long-running campaigns are not archived.
///
/// Requires `operator_secret_key` to belong to an address already granted
/// operator status via the contract's `add_operator` (creator-gated).
pub fn invoke_bump_storage(
    contract_id: &str,
    operator_secret_key: &str,
    network: &str,
) -> Result<()> {
    KeyManager::validate_secret_key(operator_secret_key)?;
    let operator_public_key = derive_public_key(operator_secret_key)?;

    println!("🔄 Invoking bump_storage_as_operator");
    println!("Contract: {}", contract_id);
    println!("Operator: {}", operator_public_key);
    println!("Network:  {}", network);

    // The operator secret is passed to the `stellar` child process via the
    // `STELLAR_ACCOUNT` env var rather than `--source-account <secret>` on
    // argv — process argv is world-readable via `ps` on shared hosts,
    // whereas a child process's environment is not.
    let status = Command::new("stellar")
        .env("STELLAR_ACCOUNT", operator_secret_key)
        .args([
            "contract",
            "invoke",
            "--id",
            contract_id,
            "--network",
            network,
            "--",
            "bump_storage_as_operator",
            "--operator",
            &operator_public_key,
        ])
        .status()
        .context("Failed to run the `stellar` CLI — is it installed? See CONTRIBUTING.md setup")?;

    if !status.success() {
        anyhow::bail!(
            "`stellar contract invoke` exited with status {}. \
             Confirm the operator was granted via add_operator and the contract ID is correct.",
            status
        );
    }

    println!("✅ bump_storage_as_operator invoked successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_public_key_matches_known_pair() {
        // Verified keypair generated via `stellar keys generate` — this
        // secret/public pair is test-only and holds no funds.
        let secret = "SC3OQOXRINSTIGJRVE2RANJ53OM2XCQL4XAK3XOZELKTKR63W7SMSKGN";
        let public = derive_public_key(secret).expect("derivation should succeed");
        assert_eq!(
            public,
            "GCI2RVCRJ5EJTXSEMU5DZCO57IUDNVBZQDITFSBFVMRYNK5WPIBZA7XJ"
        );
    }

    #[test]
    fn test_derive_public_key_rejects_malformed_secret() {
        assert!(derive_public_key("not-a-real-secret-key").is_err());
    }

    #[test]
    fn test_derive_public_key_rejects_public_key_input() {
        // A 'G...' address is not a valid seed — must be rejected, not silently
        // misinterpreted.
        let public_looking = "GCI2RVCRJ5EJTXSEMU5DZCO57IUDNVBZQDITFSBFVMRYNK5WPIBZA7XJ";
        assert!(derive_public_key(public_looking).is_err());
    }
}
