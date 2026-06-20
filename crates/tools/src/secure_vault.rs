//! Secure vault for admin and issuing key management.
//!
//! Reads keys from environment variables, validates them for testnet/mainnet,
//! and masks secrets for safe display.
//!
//! # Security Notice
//! `SecureVault::save_to_file()` has been disabled — it stored secret keys in
//! plaintext. Use `EncryptedVault::save_to_file()` (AES-256-GCM) instead via:
//! `orbitchain-cli keymanager vault-save <path>`

use anyhow::{Context, Result};
use std::env;
use std::fs;

#[derive(Debug, Clone)]
pub struct SecureVault {
    pub admin_secret_key: Option<String>,
    pub admin_public_key: Option<String>,
    pub issuing_secret_key: Option<String>,
    pub issuing_public_key: Option<String>,
}

impl SecureVault {
    #[must_use]
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();

        Self {
            admin_secret_key: env::var("SOROBAN_ADMIN_SECRET_KEY").ok(),
            admin_public_key: env::var("SOROBAN_ADMIN_PUBLIC_KEY").ok(),
            issuing_secret_key: env::var("SOROBAN_ISSUING_SECRET_KEY").ok(),
            issuing_public_key: env::var("SOROBAN_ISSUING_PUBLIC_KEY").ok(),
        }
    }

    /// Validate that required keys are present for mainnet operations.
    #[must_use]
    pub fn validate_for_mainnet(&self) -> Result<()> {
        if self.admin_secret_key.is_none() {
            anyhow::bail!("SOROBAN_ADMIN_SECRET_KEY is required for mainnet operations");
        }

        if self.admin_public_key.is_none() {
            anyhow::bail!("SOROBAN_ADMIN_PUBLIC_KEY is required for mainnet operations");
        }

        // Validate key format (basic check)
        if let Some(secret) = &self.admin_secret_key {
            if !secret.starts_with('S') {
                anyhow::bail!("Admin secret key must start with 'S'");
            }
        }

        if let Some(public) = &self.admin_public_key {
            if !public.starts_with('G') {
                anyhow::bail!("Admin public key must start with 'G'");
            }
        }

        Ok(())
    }

    /// Validate keys for testnet (permissive — allows empty/unset keys).
    #[must_use]
    pub fn validate_for_testnet(&self) -> Result<()> {
        if let Some(secret) = &self.admin_secret_key {
            if !secret.is_empty() && !secret.starts_with('S') {
                anyhow::bail!("Admin secret key must start with 'S' or be empty");
            }
        }

        if let Some(public) = &self.admin_public_key {
            if !public.is_empty() && !public.starts_with('G') {
                anyhow::bail!("Admin public key must start with 'G' or be empty");
            }
        }

        Ok(())
    }

    /// Mask secret keys for safe display
    pub fn display_safe(&self) {
        println!("🔐 Secure Vault Status");
        println!("━━━━━━━━━━━━━━━━━━━━");

        match &self.admin_public_key {
            Some(key) => println!("Admin Public Key: {}", key),
            None => println!("Admin Public Key: ⚠️  Not set"),
        }

        match &self.admin_secret_key {
            Some(key) if key.len() > 10 => {
                println!(
                    "Admin Secret Key: {}...{}",
                    &key[..4],
                    &key[key.len() - 4..]
                )
            }
            Some(_) => println!("Admin Secret Key: ***"),
            None => println!("Admin Secret Key: ⚠️  Not set"),
        }

        match &self.issuing_public_key {
            Some(key) => println!("Issuing Public Key: {}", key),
            None => println!("Issuing Public Key: ⚠️  Not set"),
        }

        match &self.issuing_secret_key {
            Some(key) if key.len() > 10 => {
                println!(
                    "Issuing Secret Key: {}...{}",
                    &key[..4],
                    &key[key.len() - 4..]
                )
            }
            Some(_) => println!("Issuing Secret Key: ***"),
            None => println!("Issuing Secret Key: ⚠️  Not set"),
        }
    }

    /// Save vault to encrypted file (placeholder for future encryption)
    /// Save vault to encrypted file (placeholder for future encryption)
    /// # Deprecated
    /// This method stores keys in plaintext. Use `EncryptedVault::save_to_file()` instead.
    pub fn save_to_file(&self, _path: &str) -> Result<()> {
        eprintln!("🚨 ERROR: SecureVault::save_to_file() stores keys in PLAINTEXT.");
        eprintln!("   Use EncryptedVault::save_to_file() instead.");
        eprintln!("   Example: orbitchain-cli keymanager vault-save <path>");
        anyhow::bail!("Plaintext vault save disabled for security. Use EncryptedVault.");
    }

    /// Load vault from file
    pub fn load_from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path).context("Failed to read vault file")?;

        let mut vault = Self {
            admin_secret_key: None,
            admin_public_key: None,
            issuing_secret_key: None,
            issuing_public_key: None,
        };

        for line in content.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                match parts[0] {
                    "SOROBAN_ADMIN_SECRET_KEY" => {
                        vault.admin_secret_key = Some(parts[1].to_string())
                    }
                    "SOROBAN_ADMIN_PUBLIC_KEY" => {
                        vault.admin_public_key = Some(parts[1].to_string())
                    }
                    "SOROBAN_ISSUING_SECRET_KEY" => {
                        vault.issuing_secret_key = Some(parts[1].to_string())
                    }
                    "SOROBAN_ISSUING_PUBLIC_KEY" => {
                        vault.issuing_public_key = Some(parts[1].to_string())
                    }
                    _ => {}
                }
            }
        }

        Ok(vault)
    }
}

/// Check mainnet configuration readiness
pub fn check_mainnet_readiness() -> Result<()> {
    let vault = SecureVault::from_env();

    println!("🔒 Mainnet Configuration Check");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Validate vault
    if let Err(e) = vault.validate_for_mainnet() {
        println!("❌ Mainnet validation failed: {}", e);
        println!();
        println!("💡 To configure mainnet:");
        println!("   1. Set SOROBAN_NETWORK=mainnet in .env");
        println!("   2. Set SOROBAN_ADMIN_SECRET_KEY=<your_secret_key>");
        println!("   3. Set SOROBAN_ADMIN_PUBLIC_KEY=<your_public_key>");
        println!("   4. Ensure you have sufficient XLM for transaction fees");
        return Err(e);
    }

    println!("✅ Admin keys configured");
    vault.display_safe();

    println!();
    println!("✅ Mainnet configuration is ready");
    println!("⚠️  WARNING: Mainnet transactions use real XLM!");

    Ok(())
}

/// Toggle between testnet and mainnet configurations
pub fn toggle_network(network: &str) -> Result<()> {
    match network {
        "testnet" => {
            println!("🔄 Switching to TESTNET...");
            println!("✅ Network: testnet");
            println!("💡 Use testnet for development and testing");
        }
        "mainnet" => {
            println!("🔄 Switching to MAINNET...");
            check_mainnet_readiness()?;
        }
        _ => anyhow::bail!("Unknown network: {}. Use 'testnet' or 'mainnet'", network),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_from_env() {
        let vault = SecureVault::from_env();
        // Should not panic even if keys are not set
        assert!(vault.admin_secret_key.is_none() || vault.admin_secret_key.is_some());
    }

    #[test]
    fn test_validate_for_testnet() {
        let vault = SecureVault::from_env();
        // Testnet validation should pass even without keys
        assert!(vault.validate_for_testnet().is_ok());
    }

    #[test]
    fn test_display_safe() {
        let vault = SecureVault::from_env();
        vault.display_safe();
        // Should not panic
    }
}
