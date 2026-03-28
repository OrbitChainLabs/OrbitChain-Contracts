//! Asset Configuration Storage - Optimized Version
//!
//! Provides on-chain storage for asset configuration and admin management.
//! 
//! # Optimizations:
//! - Uses Symbol instead of String for asset codes (saves ~20-40 bytes per asset)
//! - Compact storage keys
//! - Reduced String allocations

use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec, Symbol};
use crate::rbac::Rbac;

/// Storage keys for asset configuration (optimized with symbols)
#[contracttype]
pub enum AssetStorageKey {
    /// List of supported asset codes (stored as Symbols)
    SupportedAssets,
    /// Admin address for asset management
    AssetAdmin,
    /// Whether asset management is initialized
    Initialized,
}

/// Asset configuration manager
pub struct AssetConfig;

impl AssetConfig {
    /// Initialize the asset configuration with default supported assets
    pub fn init(env: &Env, admin: &Address) {
        if Self::is_initialized(env) {
            return; // Already initialized
        }
        
        // Set admin
        env.storage().instance().set(&AssetStorageKey::AssetAdmin, admin);
        
        // Initialize with default supported assets using Symbols (more efficient)
        let default_assets = vec![
            symbol_short!("XLM"),
            symbol_short!("USDC"),
            symbol_short!("NGNT"),
            symbol_short!("USDT"),
            symbol_short!("EURT"),
        ];
        
        env.storage().instance().set(&AssetStorageKey::SupportedAssets, &default_assets);
        env.storage().instance().set(&AssetStorageKey::Initialized, &true);
    }
    
    /// Check if asset configuration is initialized
    pub fn is_initialized(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&AssetStorageKey::Initialized)
            .unwrap_or(false)
    }
    
    /// Get the list of supported assets (as Symbols)
    pub fn get_supported_assets_symbols(env: &Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&AssetStorageKey::SupportedAssets)
            .unwrap_or_else(|| {
                // Return default assets if not initialized
                vec![
                    symbol_short!("XLM"),
                    symbol_short!("USDC"),
                    symbol_short!("NGNT"),
                    symbol_short!("USDT"),
                    symbol_short!("EURT"),
                ]
            })
    }
    
    /// Get the list of supported assets (as Strings for API compatibility)
    pub fn get_supported_assets(env: &Env) -> Vec<String> {
        let symbols = Self::get_supported_assets_symbols(env);
        let mut strings = Vec::new(env);
        for symbol in symbols.iter() {
            strings.push_back(String::from_str(env, &symbol.to_string()));
        }
        strings
    }
    
    /// Check if an asset is supported (optimized version using Symbol)
    pub fn is_asset_supported_optimized(env: &Env, asset_code: &str) -> bool {
        // Convert to Symbol for comparison (faster than String)
        let asset_symbol = if asset_code.len() <= 9 {
            Symbol::try_from_small_str(asset_code).unwrap_or(symbol_short!("UNKNOWN"))
        } else {
            return false; // Asset codes > 9 chars not supported
        };
        
        let assets = Self::get_supported_assets_symbols(env);
        assets.contains(&asset_symbol)
    }
    
    /// Check if an asset is supported (legacy API for compatibility)
    pub fn is_asset_supported(env: &Env, asset_code: &str) -> bool {
        Self::is_asset_supported_optimized(env, asset_code)
    }
    
    /// Add a new supported asset (admin only) - optimized version
    pub fn add_asset(env: &Env, caller: &Address, asset_code: &str) -> Result<(), &'static str> {
        // Verify admin
        Self::verify_admin(env, caller)?;
        
        // Validate asset code length for Symbol compatibility
        if asset_code.is_empty() || asset_code.len() > 9 {
            return Err("Invalid asset code length");
        }
        
        let asset_symbol = Symbol::try_from_small_str(asset_code)
            .map_err(|_| "Invalid asset code")?;
        
        let mut assets = Self::get_supported_assets_symbols(env);
        
        // Check if already supported
        if assets.contains(&asset_symbol) {
            return Err("Asset already supported");
        }
        
        assets.push_back(asset_symbol);
        env.storage().instance().set(&AssetStorageKey::SupportedAssets, &assets);
        Ok(())
    }
    
    /// Remove a supported asset (admin only) - optimized version
    pub fn remove_asset(env: &Env, caller: &Address, asset_code: &str) -> Result<(), &'static str> {
        // Verify admin
        Self::verify_admin(env, caller)?;
        
        // Validate and convert to Symbol
        if asset_code.is_empty() || asset_code.len() > 9 {
            return Err("Invalid asset code length");
        }
        
        let asset_symbol = Symbol::try_from_small_str(asset_code)
            .map_err(|_| "Invalid asset code")?;
        
        let mut assets = Self::get_supported_assets_symbols(env);
        
        // Cannot remove if not in list
        if !assets.contains(&asset_symbol) {
            return Err("Asset not in supported list");
        }
        
        // Remove the asset
        let mut new_assets = Vec::new(env);
        for i in 0..assets.len() {
            if let Some(asset) = assets.get(i) {
                if asset != asset_symbol {
                    new_assets.push_back(asset);
                }
            }
        }
        
        env.storage().instance().set(&AssetStorageKey::SupportedAssets, &new_assets);
        Ok(())
    }
    
    /// Update the asset admin (admin only)
    pub fn update_admin(env: &Env, caller: &Address, new_admin: &Address) -> Result<(), &'static str> {
        Rbac::update_admin(env, caller, new_admin);
        Ok(())
    }
    
    /// Get the current admin
    pub fn get_admin(env: &Env) -> Option<Address> {
        Rbac::get_admin(env)
    }

    /// Verify that the caller is the admin and has authorized the operation
    fn verify_admin(env: &Env, caller: &Address) -> Result<(), &'static str> {
        Rbac::require_admin_auth(env, caller);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    
    #[test]
    fn test_asset_config_initialization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        
        AssetConfig::init(&env, &admin);
        
        assert!(AssetConfig::is_initialized(&env));
        assert_eq!(AssetConfig::get_admin(&env), Some(admin));
        
        let assets = AssetConfig::get_supported_assets(&env);
        assert_eq!(assets.len(), 5);
    }
    
    #[test]
    fn test_add_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        AssetConfig::init(&env, &admin);
        
        // Add new asset
        let result = AssetConfig::add_asset(&env, &admin, "BTC");
        assert!(result.is_ok());
        
        // Verify it was added
        assert!(AssetConfig::is_asset_supported(&env, "BTC"));
    }
    
    #[test]
    fn test_remove_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        AssetConfig::init(&env, &admin);
        
        // Remove an asset
        let result = AssetConfig::remove_asset(&env, &admin, "EURT");
        assert!(result.is_ok());
        
        // Verify it was removed
        assert!(!AssetConfig::is_asset_supported(&env, "EURT"));
    }
    
    #[test]
    #[should_panic(expected = "Unauthorized: caller is not admin")]
    fn test_unauthorized_access() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        AssetConfig::init(&env, &admin);
        
        // Try to add asset as non-admin - should panic now
        let _ = AssetConfig::add_asset(&env, &other, "BTC");
    }
}
