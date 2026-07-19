use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub name: String,
    pub rpc_url: String,
    pub horizon_url: String,
    pub network_passphrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub network: String,
    pub testnet: NetworkConfig,
    pub mainnet: NetworkConfig,
    pub admin_public_key: Option<String>,
    pub issuing_public_key: Option<String>,
}

impl EnvironmentConfig {
    #[must_use]
    pub fn default_for_testnet() -> Self {
        Self {
            network: "testnet".to_string(),
            testnet: NetworkConfig {
                name: "testnet".to_string(),
                rpc_url: "https://soroban-testnet.stellar.org:443".to_string(),
                horizon_url: "https://horizon-testnet.stellar.org".to_string(),
                network_passphrase: "Test SDF Network ; September 2015".to_string(),
            },
            mainnet: NetworkConfig {
                name: "mainnet".to_string(),
                rpc_url: "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string(),
                horizon_url: "https://horizon.stellar.org".to_string(),
                network_passphrase: "Public Global Stellar Network ; September 2015".to_string(),
            },
            admin_public_key: None,
            issuing_public_key: None,
        }
    }

    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        let network = env::var("SOROBAN_NETWORK").unwrap_or_else(|_| "testnet".to_string());

        let testnet = NetworkConfig {
            name: "testnet".to_string(),
            rpc_url: env::var("SOROBAN_TESTNET_RPC_URL")
                .unwrap_or_else(|_| "https://soroban-testnet.stellar.org:443".to_string()),
            horizon_url: env::var("SOROBAN_TESTNET_HORIZON_URL")
                .unwrap_or_else(|_| "https://horizon-testnet.stellar.org".to_string()),
            network_passphrase: env::var("SOROBAN_TESTNET_PASSPHRASE")
                .unwrap_or_else(|_| "Test SDF Network ; September 2015".to_string()),
        };

        let mainnet = NetworkConfig {
            name: "mainnet".to_string(),
            rpc_url: env::var("SOROBAN_MAINNET_RPC_URL")
                .unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string()),
            horizon_url: env::var("SOROBAN_MAINNET_HORIZON_URL")
                .unwrap_or_else(|_| "https://horizon.stellar.org".to_string()),
            network_passphrase: env::var("SOROBAN_MAINNET_PASSPHRASE")
                .unwrap_or_else(|_| "Public Global Stellar Network ; September 2015".to_string()),
        };

        let admin_public_key = env::var("SOROBAN_ADMIN_PUBLIC_KEY").ok();
        let issuing_public_key = env::var("SOROBAN_ISSUING_PUBLIC_KEY").ok();

        Ok(Self {
            network,
            testnet,
            mainnet,
            admin_public_key,
            issuing_public_key,
        })
    }

    pub fn get_active_network(&self) -> Result<NetworkConfig> {
        match self.network.as_str() {
            "testnet" => Ok(self.testnet.clone()),
            "mainnet" => Ok(self.mainnet.clone()),
            _ => anyhow::bail!("Unknown network: {}", self.network),
        }
    }

    /// Validate configuration — checks active network URLs and passphrase.
    pub fn validate(&self) -> Result<()> {
        let active = self.get_active_network()?;

        if active.rpc_url.is_empty() {
            anyhow::bail!("RPC URL is required");
        }

        if active.horizon_url.is_empty() {
            anyhow::bail!("Horizon URL is required");
        }

        if active.network_passphrase.is_empty() {
            anyhow::bail!("Network passphrase is required");
        }

        Ok(())
    }

    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(path, content).context("Failed to write config file")?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path).context("Failed to read config file")?;
        let config: Self = toml::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }
}

pub fn check_testnet_connection() -> Result<()> {
    let config = EnvironmentConfig::from_env()?;
    let testnet = config.testnet.clone();

    println!("🔍 Testing testnet connection...");
    println!("RPC URL: {}", testnet.rpc_url);
    println!("Horizon URL: {}", testnet.horizon_url);
    println!("Network Passphrase: {}", testnet.network_passphrase);

    // Validate configuration
    config.validate()?;

    println!("✅ Testnet configuration is valid");
    println!("💡 To test connection, ensure you have:");
    println!("   1. Installed Soroban CLI: cargo install soroban-cli");
    println!(
        "   2. Generated a testnet keypair: soroban keys generate test_account --network testnet"
    );
    println!("   3. Funded account from: https://laboratory.stellar.org/#account-creator?network=testnet");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EnvironmentConfig::from_env();
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.network, "testnet");
        assert_eq!(config.testnet.name, "testnet");
        assert_eq!(config.mainnet.name, "mainnet");
    }

    #[test]
    fn test_get_active_network() {
        let config = EnvironmentConfig::from_env().unwrap();
        let active = config.get_active_network();
        assert!(active.is_ok());

        let active = active.unwrap();
        assert_eq!(active.name, "testnet");
    }

    #[test]
    fn test_validate_config() {
        let config = EnvironmentConfig::from_env().unwrap();
        assert!(config.validate().is_ok());
    }
}
