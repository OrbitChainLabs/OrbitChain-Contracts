//! Environment-based encrypted vault for secure key storage.
//!
//! Keys are encrypted with AES-256-GCM at rest and only decrypted when needed.
//! Supports saving/loading vault files with restrictive Unix permissions (0600).

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::env;
use std::fs;

use crate::key_manager::KeyManager;

/// Environment-based encrypted vault for secure key storage
/// Keys are encrypted at rest and only decrypted when needed
#[derive(Debug)]
pub struct EncryptedVault {
    /// Master key manager instance
    key_manager: Option<KeyManager>,
    /// Encrypted keys stored as hex strings
    encrypted_keys: HashMap<String, String>,
    /// Public keys (safe to store unencrypted)
    public_keys: HashMap<String, String>,
}

impl EncryptedVault {
    /// Create a new empty vault
    #[must_use]
    pub fn new() -> Self {
        Self {
            key_manager: None,
            encrypted_keys: HashMap::new(),
            public_keys: HashMap::new(),
        }
    }

    /// Initialize vault with a master password
    pub fn with_password(password: &str) -> Result<Self> {
        let key_manager = KeyManager::from_password(password)?;
        Ok(Self {
            key_manager: Some(key_manager),
            encrypted_keys: HashMap::new(),
            public_keys: HashMap::new(),
        })
    }

    /// Initialize vault with a hex-encoded master key
    pub fn with_hex_key(hex_key: &str) -> Result<Self> {
        let key_manager = KeyManager::from_hex_key(hex_key)?;
        Ok(Self {
            key_manager: Some(key_manager),
            encrypted_keys: HashMap::new(),
            public_keys: HashMap::new(),
        })
    }

    /// Load vault configuration from .env file
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        // Try to get master password from environment
        let master_password = env::var("VAULT_MASTER_PASSWORD")
            .or_else(|_| env::var("VAULT_MASTER_KEY"))
            .ok();

        let mut vault = if let Some(password) = master_password {
            Self::with_password(&password)?
        } else {
            Self::new()
        };

        // Load public keys
        if let Ok(admin_pub) = env::var("SOROBAN_ADMIN_PUBLIC_KEY") {
            vault
                .public_keys
                .insert("admin_public_key".to_string(), admin_pub);
        }
        if let Ok(issuing_pub) = env::var("SOROBAN_ISSUING_PUBLIC_KEY") {
            vault
                .public_keys
                .insert("issuing_public_key".to_string(), issuing_pub);
        }

        // Load encrypted keys (stored as VAR_NAME_ENCRYPTED=hex:data format)
        if let Ok(admin_enc) = env::var("SOROBAN_ADMIN_SECRET_KEY_ENCRYPTED") {
            vault
                .encrypted_keys
                .insert("admin_secret_key".to_string(), admin_enc);
        }
        if let Ok(issuing_enc) = env::var("SOROBAN_ISSUING_SECRET_KEY_ENCRYPTED") {
            vault
                .encrypted_keys
                .insert("issuing_secret_key".to_string(), issuing_enc);
        }

        Ok(vault)
    }

    /// Store an encrypted secret key in the vault.
    pub fn store_secret_key(&mut self, key_name: &str, secret_key: &str) -> Result<()> {
        // Validate key format
        KeyManager::validate_secret_key(secret_key)?;

        // Ensure key manager is initialized
        if self.key_manager.is_none() {
            anyhow::bail!("Vault key manager not initialized. Call with_password() first.");
        }

        // Encrypt and store
        let key_manager = self.key_manager.as_ref().unwrap();
        let encrypted_hex = key_manager.export_encrypted(secret_key)?;
        self.encrypted_keys
            .insert(key_name.to_string(), encrypted_hex);

        Ok(())
    }

    /// Retrieve and decrypt a secret key from the vault.
    pub fn retrieve_secret_key(&self, key_name: &str) -> Result<String> {
        // Get encrypted key
        let encrypted_hex = self
            .encrypted_keys
            .get(key_name)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in vault", key_name))?;

        // Ensure key manager is initialized
        if self.key_manager.is_none() {
            anyhow::bail!("Vault key manager not initialized. Cannot decrypt keys.");
        }

        // Decrypt
        let key_manager = self.key_manager.as_ref().unwrap();
        let encrypted = key_manager.import_encrypted(encrypted_hex)?;
        key_manager.decrypt_key(&encrypted)
    }

    /// Store a public key (unencrypted).
    pub fn store_public_key(&mut self, key_name: &str, public_key: &str) -> Result<()> {
        KeyManager::validate_public_key(public_key)?;
        self.public_keys
            .insert(key_name.to_string(), public_key.to_string());
        Ok(())
    }

    /// Retrieve a public key from the vault.
    pub fn retrieve_public_key(&self, key_name: &str) -> Result<String> {
        self.public_keys
            .get(key_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Public key '{}' not found in vault", key_name))
    }

    /// Save vault to encrypted file
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let mut content = String::from("# Encrypted Vault Configuration\n");
        content.push_str("# WARNING: Keep this file secure!\n\n");

        // Save public keys
        content.push_str("# Public Keys (unencrypted)\n");
        for (name, key) in &self.public_keys {
            content.push_str(&format!("{}={}\n", name.to_uppercase(), key));
        }

        content.push_str("\n# Encrypted Secret Keys\n");
        for (name, encrypted) in &self.encrypted_keys {
            content.push_str(&format!(
                "{}_ENCRYPTED={}\n",
                name.to_uppercase(),
                encrypted
            ));
        }

        fs::write(path, content).context("Failed to write vault file")?;

        // Set restrictive file permissions (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms)?;
        }

        println!("✅ Vault saved to {}", path);
        Ok(())
    }

    /// Load vault from encrypted file
    pub fn load_from_file(path: &str, password: &str) -> Result<Self> {
        let content = fs::read_to_string(path).context("Failed to read vault file")?;
        let mut vault = Self::with_password(password)?;

        for line in content.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                let key_name = parts[0].to_lowercase();
                let value = parts[1];

                if key_name.ends_with("_encrypted") {
                    let name = key_name.trim_end_matches("_encrypted").to_string();
                    vault.encrypted_keys.insert(name, value.to_string());
                } else {
                    vault.public_keys.insert(key_name, value.to_string());
                }
            }
        }

        Ok(vault)
    }

    /// Display vault status (safe, no key exposure)
    pub fn display_status(&self) {
        println!("🔐 Encrypted Vault Status");
        println!("━━━━━━━━━━━━━━━━━━━━━━");

        if self.key_manager.is_some() {
            println!("✅ Key Manager: Initialized");
        } else {
            println!("⚠️  Key Manager: NOT initialized (cannot decrypt keys)");
        }

        println!("\nPublic Keys:");
        for name in self.public_keys.keys() {
            println!("  ✓ {}", name);
        }

        if self.public_keys.is_empty() {
            println!("  (none)");
        }

        println!("\nEncrypted Secret Keys:");
        for name in self.encrypted_keys.keys() {
            println!("  🔒 {} (encrypted)", name);
        }

        if self.encrypted_keys.is_empty() {
            println!("  (none)");
        }
    }

    /// Export vault configuration as environment variables.
    #[must_use]
    pub fn export_to_env_vars(&self) -> Vec<(String, String)> {
        let mut vars = Vec::new();

        for (name, key) in &self.public_keys {
            vars.push((format!("{}_PUB", name.to_uppercase()), key.clone()));
        }

        for (name, encrypted) in &self.encrypted_keys {
            vars.push((
                format!("{}_ENCRYPTED", name.to_uppercase()),
                encrypted.clone(),
            ));
        }

        vars
    }
}

impl Default for EncryptedVault {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_empty_vault() {
        let vault = EncryptedVault::new();
        assert!(vault.key_manager.is_none());
        assert!(vault.encrypted_keys.is_empty());
        assert!(vault.public_keys.is_empty());
    }

    #[test]
    fn test_vault_with_password() -> Result<()> {
        let vault = EncryptedVault::with_password("test_password")?;
        assert!(vault.key_manager.is_some());
        Ok(())
    }

    #[test]
    fn test_store_and_retrieve_secret_key() -> Result<()> {
        let mut vault = EncryptedVault::with_password("secure_password")?;
        let secret_key = "SAVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCVLG5";

        vault.store_secret_key("admin_secret_key", secret_key)?;
        let retrieved = vault.retrieve_secret_key("admin_secret_key")?;

        assert_eq!(retrieved, secret_key);
        Ok(())
    }

    #[test]
    fn test_store_and_retrieve_public_key() -> Result<()> {
        let mut vault = EncryptedVault::new();
        let public_key = "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W";

        vault.store_public_key("admin_public_key", public_key)?;
        let retrieved = vault.retrieve_public_key("admin_public_key")?;

        assert_eq!(retrieved, public_key);
        Ok(())
    }

    #[test]
    fn test_retrieve_nonexistent_key_fails() -> Result<()> {
        let vault = EncryptedVault::new();
        let result = vault.retrieve_secret_key("nonexistent");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_save_and_load_vault() -> Result<()> {
        let temp_path = "/tmp/test_vault.enc";

        let mut vault = EncryptedVault::with_password("test_password")?;
        vault.store_secret_key(
            "admin_secret_key",
            "SAVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCVLG5",
        )?;
        vault.store_public_key(
            "admin_public_key",
            "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W",
        )?;

        vault.save_to_file(temp_path)?;

        let loaded_vault = EncryptedVault::load_from_file(temp_path, "test_password")?;
        let secret = loaded_vault.retrieve_secret_key("admin_secret_key")?;
        let public = loaded_vault.retrieve_public_key("admin_public_key")?;

        assert_eq!(
            secret,
            "SAVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCUKRKFIVCVLG5"
        );
        assert_eq!(
            public,
            "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W"
        );

        // Cleanup
        let _ = fs::remove_file(temp_path);
        Ok(())
    }

    #[test]
    fn test_export_to_env_vars() -> Result<()> {
        let mut vault = EncryptedVault::new();
        vault.store_public_key(
            "admin_public_key",
            "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W",
        )?;

        let vars = vault.export_to_env_vars();
        assert!(!vars.is_empty());
        assert!(vars.iter().any(|(k, _)| k.contains("ADMIN_PUBLIC_KEY")));

        Ok(())
    }
}
