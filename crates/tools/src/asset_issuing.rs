//! Stellar asset issuing and trustline management.
//!
//! Handles asset configuration from environment, trustline establishment,
//! asset issuance to recipients, and issuing readiness validation.

use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct AssetConfig {
    pub code: String,
    pub name: String,
    pub issuing_secret_key: String,
    pub issuing_public_key: String,
    pub distributor_public_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrustlineConfig {
    pub asset_code: String,
    pub asset_issuer: String,
    pub holder_public_key: String,
}

impl AssetConfig {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        let code = env::var("ASSET_CODE").unwrap_or_else(|_| "ORBIT".to_string());

        let name = env::var("ASSET_NAME").unwrap_or_else(|_| "OrbitChain Token".to_string());

        let issuing_secret_key = env::var("SOROBAN_ISSUING_SECRET_KEY")
            .context("SOROBAN_ISSUING_SECRET_KEY is required")?;

        let issuing_public_key = env::var("SOROBAN_ISSUING_PUBLIC_KEY")
            .context("SOROBAN_ISSUING_PUBLIC_KEY is required")?;

        let distributor_public_key = env::var("SOROBAN_DISTRIBUTOR_PUBLIC_KEY").ok();

        Ok(Self {
            code,
            name,
            issuing_secret_key,
            issuing_public_key,
            distributor_public_key,
        })
    }

    /// Validate asset configuration.
    pub fn validate(&self) -> Result<()> {
        if self.code.is_empty() || self.code.len() > 12 {
            anyhow::bail!("Asset code must be 1-12 characters");
        }

        if !self.issuing_secret_key.starts_with('S') {
            anyhow::bail!("Issuing secret key must start with 'S'");
        }

        if !self.issuing_public_key.starts_with('G') {
            anyhow::bail!("Issuing public key must start with 'G'");
        }

        Ok(())
    }

    /// Display asset configuration (masks secret key)
    pub fn display(&self) {
        println!("🪙 Asset Configuration");
        println!("━━━━━━━━━━━━━━━━━━━━");
        println!("Asset Code: {}", self.code);
        println!("Asset Name: {}", self.name);
        println!("Issuer Public Key: {}", self.issuing_public_key);

        if self.issuing_secret_key.len() > 10 {
            println!(
                "Issuer Secret Key: {}...{}",
                &self.issuing_secret_key[..4],
                &self.issuing_secret_key[self.issuing_secret_key.len() - 4..]
            );
        } else {
            println!("Issuer Secret Key: ***");
        }

        if let Some(distributor) = &self.distributor_public_key {
            println!("Distributor Public Key: {}", distributor);
        } else {
            println!("Distributor Public Key: ⚠️  Not set");
        }
    }

    /// Generate asset canonical identifier
    pub fn canonical_id(&self) -> String {
        format!("{}:{}", self.code, self.issuing_public_key)
    }
}

/// Generate a new issuing keypair
pub fn generate_issuing_keypair() -> Result<(String, String)> {
    // In a real implementation, this would use the stellar-strkey crate
    // For now, we provide guidance on how to generate keys

    println!("🔑 Generating Issuing Keypair");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("To generate a new keypair, run:");
    println!("  soroban keys generate issuing_account --network testnet");
    println!();
    println!("Or use the Stellar Laboratory:");
    println!("  https://laboratory.stellar.org/#account-creator");
    println!();
    println!("Then set in your .env file:");
    println!("  SOROBAN_ISSUING_SECRET_KEY=S...");
    println!("  SOROBAN_ISSUING_PUBLIC_KEY=G...");

    Ok((
        "S_PLACEHOLDER_REPLACE_WITH_YOUR_SECRET_KEY".to_string(),
        "G_PLACEHOLDER_REPLACE_WITH_YOUR_PUBLIC_KEY".to_string(),
    ))
}

/// Establish trustlines for an asset
pub fn establish_trustline(config: &TrustlineConfig, network: &str) -> Result<()> {
    println!("🔗 Establishing Trustline");
    println!("━━━━━━━━━━━━━━━━━━━━━━");
    println!("Asset: {}:{}", config.asset_code, config.asset_issuer);
    println!("Holder: {}", config.holder_public_key);
    println!("Network: {}", network);
    println!();

    // Validate configuration
    if config.asset_code.is_empty() {
        anyhow::bail!("Asset code is required");
    }

    if !config.asset_issuer.starts_with('G') {
        anyhow::bail!("Asset issuer must be a valid public key starting with 'G'");
    }

    if !config.holder_public_key.starts_with('G') {
        anyhow::bail!("Holder public key must start with 'G'");
    }

    println!("✅ Trustline configuration is valid");
    println!();
    println!("💡 To establish a trustline, you need to:");
    println!("   1. Use the Stellar CLI or Laboratory");
    println!("   2. Submit a 'change_trust' operation");
    println!("   3. Pay the trustline reserve (0.5 XLM)");
    println!();
    println!("Example using Soroban CLI:");
    println!("  soroban contract invoke \\");
    println!("    --network {} \\", network);
    println!("    --source-account holder \\");
    println!(
        "    -- <contract_id> change_trust --asset '{}:{}'",
        config.asset_code, config.asset_issuer
    );

    Ok(())
}

/// Issue assets to a recipient
pub fn issue_asset(
    asset_config: &AssetConfig,
    recipient: &str,
    amount: f64,
    network: &str,
) -> Result<()> {
    println!("💰 Issuing Assets");
    println!("━━━━━━━━━━━━━━━━");
    println!(
        "Asset: {}:{}",
        asset_config.code, asset_config.issuing_public_key
    );
    println!("Recipient: {}", recipient);
    println!("Amount: {}", amount);
    println!("Network: {}", network);
    println!();

    // Validate
    asset_config.validate()?;

    if !recipient.starts_with('G') {
        anyhow::bail!("Recipient must be a valid public key starting with 'G'");
    }

    if amount <= 0.0 {
        anyhow::bail!("Amount must be greater than 0");
    }

    println!("✅ Asset issuance configuration is valid");
    println!();
    println!("💡 To issue assets, you need to:");
    println!("   1. Ensure recipient has established trustline");
    println!("   2. Use the Stellar CLI or Laboratory");
    println!("   3. Submit a 'payment' operation from issuing account");
    println!();
    println!("Example using Stellar CLI:");
    println!("  stellar payment create \\");
    println!("    --source-account issuing_account \\");
    println!("    --destination {} \\", recipient);
    println!("    --amount {} \\", amount);
    println!(
        "    --asset '{}:{}' \\",
        asset_config.code, asset_config.issuing_public_key
    );
    println!("    --network {}", network);

    Ok(())
}

/// Check if asset issuing is ready
pub fn check_issuing_readiness() -> Result<()> {
    println!("🔍 Asset Issuing Readiness Check");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let asset_config = AssetConfig::from_env()?;

    // Display current config
    asset_config.display();
    println!();

    // Validate
    match asset_config.validate() {
        Ok(_) => {
            println!("✅ Asset configuration is valid");
            println!();
            println!("💡 Next steps:");
            println!("   1. Fund the issuing account with XLM for reserves");
            println!("   2. Establish trustlines for recipients");
            println!("   3. Issue assets to distributors or users");
            Ok(())
        }
        Err(e) => {
            println!("❌ Asset configuration validation failed: {}", e);
            println!();
            println!("💡 To configure asset issuing:");
            println!("   1. Set ASSET_CODE in .env (1-12 characters)");
            println!("   2. Set SOROBAN_ISSUING_SECRET_KEY in .env");
            println!("   3. Set SOROBAN_ISSUING_PUBLIC_KEY in .env");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_config_from_env() {
        // This will use defaults from .env.example if .env doesn't exist
        let result = AssetConfig::from_env();
        // May fail if issuing keys are not set, which is expected
        if let Ok(config) = result {
            assert!(!config.code.is_empty());
            assert!(config.code.len() <= 12);
            assert!(
                config.issuing_public_key.starts_with('G'),
                "issuing_public_key must start with 'G' (got {})",
                config.issuing_public_key
            );
            assert!(
                config.issuing_secret_key.starts_with('S'),
                "issuing_secret_key must start with 'S'"
            );
            config.validate().expect("env-loaded config must validate");
        }
    }

    #[test]
    fn test_canonical_id() {
        let config = AssetConfig {
            code: "TEST".to_string(),
            name: "Test Asset".to_string(),
            issuing_secret_key: "STEST123".to_string(),
            issuing_public_key: "GTEST123".to_string(),
            distributor_public_key: None,
        };

        let canonical = config.canonical_id();
        assert_eq!(canonical, "TEST:GTEST123");
    }
}
